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
        format!("{} field boundaries, {metres:.0} m of walls, hedges and fences; {} farmsteads", laid.boundaries.len(), farm_buildings.len() / 2)
    }
}
