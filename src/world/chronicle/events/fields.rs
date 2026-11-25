use crate::world::chronicle::{EventCtx, Process};
use crate::world::city::{self, BuildingDef, CityFile};
use crate::world::fields::Pos;
use crate::world::noise::hash32;
use crate::world::route;
use crate::world::town::{self, country, Edge, Kind};
use glam::Vec2;
use serde::Deserialize;

/// Farmland inside an outline, away from any town: fields laid off the
/// roads the world already has (every road the routing engine laid, and the
/// roads out of earlier towns), walls and hedges built once along each
/// shared edge and kept off the roads, crops painted as regions, and a
/// farmstead by the road every 250 m or so. The same fields a `town` lays
/// round itself (`world/town/country.rs`); docs/TOWNS.md, "Fields".
#[derive(Deserialize, Debug)]
pub struct FieldsEvent {
    /// The outline of the farmland.
    area: Vec<Pos>,
    /// Region names for the fields, picked in turn (for biomes; give them
    /// `"crisp": true` so a crop stops at its wall).
    #[serde(default)]
    crops: Vec<String>,
    /// The city kit whose `town` section names the wall, rail and hedge
    /// kits and the farmhouses' materials.
    kit: String,
    /// Metres across a field, roughly.
    #[serde(default = "d_size")]
    field_size: f32,
    /// Farmhouses and barns along the roads.
    #[serde(default = "yes")]
    farmsteads: bool,
    /// The chance a field has a shed or shack in its corner (`Shacks`).
    #[serde(default)]
    shacks: Shacks,
    #[serde(default)]
    seed: u32,
}

fn d_size() -> f32 {
    70.0
}
fn yes() -> bool {
    true
}

impl Process for FieldsEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (seed, cause) = (cx.seed ^ self.seed, cx.cause);
        let lib = cx.lib;
        let Some(kit) = lib.city_kit(&self.kit) else {
            cx.warnings.push(format!("fields: no city kit called {}", self.kit));
            return "skipped".into();
        };
        let tk = &kit.town;
        let area: Vec<Vec2> = self.area.iter().map(|p| cx.pos(p)).collect();
        if area.len() < 3 {
            cx.warnings.push("fields: an area needs at least three corners".into());
            return "skipped".into();
        }
        let lo = area.iter().fold(Vec2::splat(f32::MAX), |a, &b| a.min(b)).max(Vec2::ZERO);
        let hi = area.iter().fold(Vec2::splat(f32::MIN), |a, &b| a.max(b)).min(Vec2::splat(cx.f.size - 1.0));
        // The roads in or near the area, and everything to keep walls off.
        let near_box = |p: &Vec2| p.x > lo.x - 300.0 && p.y > lo.y - 300.0 && p.x < hi.x + 300.0 && p.y < hi.y + 300.0;
        let roads: Vec<(Vec<Vec2>, f32)> = cx.state.get::<route::RoadLines>().0.iter().filter(|(l, _)| l.iter().any(near_box)).cloned().collect();
        if roads.is_empty() {
            cx.warnings.push("fields: no road runs near the area for the fields to hang off".into());
            return "skipped".into();
        }
        let keep_off: Vec<(Vec2, Vec2, f32)> = roads.iter().flat_map(|(l, h)| l.windows(2).map(move |s| (s[0], s[1], *h + 1.0))).collect();
        let f = &cx.f;
        let ground = super::town::FieldGround(f, None);
        // Farmland: inside the outline, not on water, and not on ground
        // something else already holds (a building, a yard, a road).
        let band = |p: Vec2| {
            if !town::extent::in_poly(&area, p) {
                return false;
            }
            let i = f.idx(p.x, p.y);
            f.water[i] == 0 && !f.occupied[i]
        };
        let ok_farm = |p: Vec2| band(p);
        let (farm_buildings, farm_at) = if self.farmsteads { country::farmsteads(&roads, &ok_farm, seed, &ground) } else { (Vec::new(), Vec::new()) };
        let near = |_p: Vec2| false;
        let laid = country::lay(
            &country::Farmland {
                roads,
                keep_off,
                avoid: farm_at.iter().map(|&p| (p, 14.0)).collect(),
                band: &band,
                near: &near,
                lo,
                hi,
                size: self.field_size,
                seed,
            },
            &ground,
        );

        // Crops as regions.
        let crop_ids: Vec<u8> = self.crops.iter().map(|n| cx.f.region_id(n)).collect();
        if !crop_ids.is_empty() {
            let f = &mut cx.f;
            for z in lo.y as usize..hi.y as usize {
                for x in lo.x as usize..hi.x as usize {
                    let i = z * f.n + x;
                    let k = laid.crops.at(f.centre(i));
                    if k != 255 {
                        f.region[i] = crop_ids[k as usize % crop_ids.len()];
                    }
                }
            }
        }

        // Farmhouses, barns and hedgerow oaks, drawn like a town's.
        let mut cf = CityFile::default();
        let walls = city::TownKit::walls_for(&[&tk.walls], "village");
        let roofs = city::TownKit::roofs_for(&[&tk.roofs], "village");
        let trims = tk.trim_list();
        for (k, b) in farm_buildings.iter().enumerate() {
            let pick = |l: &[String], h: u32| l[h as usize % l.len()].clone();
            cf.buildings.push(BuildingDef {
                id: ((seed as u64) << 32) | (0x8000_0000 | k as u64),
                style: if b.kind == Kind::Barn { "industrial" } else { "house" }.into(),
                poly: b.poly.iter().map(|p| [p.x, p.y]).collect(),
                holes: Vec::new(),
                height: b.floors as f32 * 3.0,
                roof_rise: 0.0,
                floor: 3.0,
                roof: "gable".into(),
                wall: pick(&walls, hash32(b.seed ^ 1)),
                roof_mat: pick(&roofs, hash32(b.seed ^ 2)),
                trim: trims[0].clone(),
                door: 0,
                ridge: b.ridge.map(|v| [v.x, v.y]),
                eave: None,
            });
        }
        let oak = tk.tree_list().pop().unwrap_or_default();
        for (p, _) in &laid.trees {
            cf.trees.push((p.x, p.y, oak.clone()));
        }
        let mut missing = std::collections::BTreeSet::new();
        let (built, _) = city::build(&mut cx.f, &cf, kit, cause, seed, |name| {
            lib.material(name).unwrap_or_else(|| {
                missing.insert(name.to_string());
                crate::world::material::STONE
            })
        });
        for m in missing {
            cx.warnings.push(format!("fields: unknown material {m}"));
        }
        cx.state.get::<city::City>().absorb(built);

        // The walls, hedges and fences.
        let mut metres = 0.0;
        for b in &laid.boundaries {
            let kit_name = match b.edge {
                Edge::FieldWall => tk.field_wall_kit(),
                Edge::Hedge => tk.hedge_kit(),
                _ => tk.rail_kit(),
            };
            let Some(k) = route::kit_index(cx, kit_name) else {
                cx.warnings.push(format!("fields: unknown kit {kit_name}"));
                continue;
            };
            let pts = town::grow::resample(&b.pts, 2.0);
            if pts.len() >= 3 {
                metres += route::lay(cx, &pts, k, 4.0).length;
            }
        }
        let (sheds, why) = place_shacks(cx, &laid.corners, &self.crops, &self.shacks, &tk.shack_list(), seed);
        format!("{} field boundaries, {metres:.0} m of walls, hedges and fences; {} farmsteads; {sheds} field sheds ({why})", laid.boundaries.len(), farm_buildings.len() / 2)
    }
}

/// How often a field has a shed or shack in a corner: one chance for every
/// field (`0.2`), or a chance by crop region with a `default` for the rest
/// (`{"default": 0.1, "kh_flax": 0.3, "pasture": 0}`). 0 is none at all.
#[derive(Deserialize, Debug, Clone)]
#[serde(untagged)]
pub enum Shacks {
    All(f32),
    ByRegion(std::collections::HashMap<String, f32>),
}

impl Default for Shacks {
    fn default() -> Self {
        Shacks::All(DEFAULT_SHACKS)
    }
}

/// The chance a field has a shed when the event says nothing.
pub const DEFAULT_SHACKS: f32 = 0.15;

impl Shacks {
    fn rate(&self, region: Option<&str>) -> f32 {
        match self {
            Shacks::All(r) => *r,
            Shacks::ByRegion(m) => region.and_then(|n| m.get(n)).or_else(|| m.get("default")).copied().unwrap_or(DEFAULT_SHACKS),
        }
    }
}

/// Sheds and shacks in the corners of fields: for each corner, by its
/// field's rate, one of the kit's `shacks` set in just inside the walls,
/// its back to the corner and its door to the field. Skipped where the
/// ground is taken (a wall, a road, a building) or too steep. How many, and
/// a word on the rest.
pub(super) fn place_shacks(cx: &mut EventCtx, corners: &[town::country::Corner], crops: &[String], rates: &Shacks, names: &[String], seed: u32) -> (usize, String) {
    let defs: Vec<usize> = names
        .iter()
        .filter_map(|n| {
            let d = cx.lib.structure(n);
            if d.is_none() {
                cx.warnings.push(format!("fields: unknown shack structure {n}"));
            }
            d
        })
        .collect();
    if defs.is_empty() {
        return (0, String::new());
    }
    let (mut crowded, mut steep) = (0, 0);
    let none = std::collections::HashMap::default();
    let mut n = 0;
    for c in corners {
        let region = (c.crop != 255 && !crops.is_empty()).then(|| crops[c.crop as usize % crops.len()].as_str());
        let rate = rates.rate(region);
        if rate <= 0.0 || (hash32(c.hash ^ seed ^ 0x5ac) % 10_000) as f32 >= rate * 10_000.0 {
            continue;
        }
        let d = defs[hash32(c.hash ^ 0x51) as usize % defs.len()];
        let sd = &cx.lib.structures[d];
        let half = (sd.size[0].max(sd.size[2]) as f32) * crate::world::gen::VOXEL * 0.5;
        // In from the corner along the diagonal, clear of the walls: a
        // narrow corner pushes it further in.
        let f = &cx.f;
        let clear_at = |at: Vec2| {
            (0..9).all(|k| {
                let q = if k == 0 {
                    at
                } else {
                    let a = k as f32 * std::f32::consts::TAU / 8.0;
                    at + Vec2::new(a.cos(), a.sin()) * (half + 0.6)
                };
                let i = f.idx(q.x, q.y);
                f.water[i] == 0 && !f.occupied[i]
            })
        };
        let Some(at) = (0..4).map(|k| c.at + c.into * (half * 1.45 + 1.5 + k as f32 * 2.5)).find(|&p| clear_at(p)) else {
            crowded += 1;
            continue;
        };
        let hs: Vec<f32> = (0..4).map(|k| {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.785;
            let q = at + Vec2::new(a.cos(), a.sin()) * half;
            f.height(q.x, q.y)
        }).collect();
        let fall = hs.iter().cloned().fold(f32::MIN, f32::max) - hs.iter().cloned().fold(f32::MAX, f32::min);
        if fall > 1.2 {
            steep += 1;
            continue;
        }
        let bearing = crate::util::bearing(c.into).to_degrees();
        if cx.place(d, at, bearing, &none, 0.0, hash32(c.hash ^ seed)).is_some() {
            n += 1;
        }
    }
    (n, format!("{} corners, {crowded} crowded, {steep} steep", corners.len()))
}
