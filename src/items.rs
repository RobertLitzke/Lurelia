//! Things to pick up. On the legacy island, gems sit in fixed places, at most
//! one per 24 m cell; in a chronicle world, the items its events left (library
//! collectibles), on the ground or buried. Either is collected by touch.

use crate::creatures::terrain::voxel_ground;
use crate::creatures::Look;
use crate::models;
use crate::util::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::world::fields::World as Land;
use crate::world::noise::{hash2, hash32, unit};
use crate::world::store::Store;
use glam::Vec3;

pub struct Gem {
    /// Legacy gems: the 24 m cell. World items: (item index, -1).
    pub cell: (i32, i32),
    pub spin: bool,
    /// What it is (a library collectible's display name), for the HUD.
    pub what: Option<String>,
}

const GEM_CELL: f32 = 24.0;

/// Legacy gems in the cells around the player; far ones are forgotten so
/// they come back when the player returns.
pub fn legacy_gems(world: &mut hecs::World, spawned: &mut HashSet<(i32, i32)>, collected: &HashSet<(i32, i32)>, store: &Store, seed: u32, sea: f32, player: Vec3) {
    let (pcx, pcz) = ((player.x / GEM_CELL).floor() as i32, (player.z / GEM_CELL).floor() as i32);
    for cz in pcz - 3..=pcz + 3 {
        for cx in pcx - 3..=pcx + 3 {
            let cell = (cx, cz);
            if spawned.contains(&cell) || collected.contains(&cell) {
                continue;
            }
            let h = hash2(cx, cz, seed ^ 0x6e6d);
            if unit(h) > 0.4 {
                spawned.insert(cell);
                continue;
            }
            let x = (cx as f32 + 0.2 + 0.6 * unit(hash32(h))) * GEM_CELL;
            let z = (cz as f32 + 0.2 + 0.6 * unit(hash32(h ^ 77))) * GEM_CELL;
            let Some((gy, _)) = voxel_ground(store, x, z, store.top_m()) else {
                continue;
            };
            spawned.insert(cell);
            if gy < sea + 0.5 {
                continue;
            }
            let pos = Vec3::new(x, gy + 0.35, z);
            world.spawn((Gem { cell, spin: true, what: None }, Look::new(models::GEM_M, pos, 0.0)));
        }
    }
    let far: Vec<(hecs::Entity, (i32, i32))> = world
        .query::<(hecs::Entity, &Gem, &Look)>()
        .iter()
        .filter(|(_, _, l)| (l.pos - player).length() > 120.0)
        .map(|(e, g, _)| (e, g.cell))
        .collect();
    for (e, cell) in far {
        let _ = world.despawn(e);
        spawned.remove(&cell);
    }
    spawned.retain(|&(cx, cz)| (cx - pcx).abs() <= 5 && (cz - pcz).abs() <= 5);
}

/// The world's items within 60 m of the player, until collected.
pub fn world_items(
    world: &mut hecs::World,
    spawned: &mut HashSet<(i32, i32)>,
    collected: &HashSet<(i32, i32)>,
    land: &Land,
    item_models: &HashMap<String, crate::worldload::ItemModel>,
    store: &Store,
    player: Vec3,
) {
    for (k, it) in land.items.iter().enumerate() {
        let key = (k as i32, -1);
        if spawned.contains(&key) || collected.contains(&key) {
            continue;
        }
        if Vec3::new(it.pos.x - player.x, 0.0, it.pos.z - player.z).length() > 60.0 {
            continue;
        }
        // Surface finds sit on the ground as loaded; buried ones stay put.
        let pos = if it.surface {
            match voxel_ground(store, it.pos.x, it.pos.z, it.pos.y + 4.0) {
                Some((gy, _)) => Vec3::new(it.pos.x, gy, it.pos.z),
                None => continue,
            }
        } else {
            it.pos
        };
        spawned.insert(key);
        let (model, spin, what) = match item_models.get(&it.kind) {
            Some(m) => (m.model, m.spin, Some(m.display.clone())),
            None => (models::GEM_M, true, None),
        };
        let yaw = unit(hash2(k as i32, 7, 0x1f)) * std::f32::consts::TAU;
        world.spawn((Gem { cell: key, spin, what }, Look::new(model, pos, yaw)));
    }
    let far: Vec<(hecs::Entity, (i32, i32))> = world
        .query::<(hecs::Entity, &Gem, &Look)>()
        .iter()
        .filter(|(_, _, l)| Vec3::new(l.pos.x - player.x, 0.0, l.pos.z - player.z).length() > 80.0)
        .map(|(e, g, _)| (e, g.cell))
        .collect();
    for (e, cell) in far {
        let _ = world.despawn(e);
        spawned.remove(&cell);
    }
}

/// Spin the gems and take the ones the player touches: their cells and names.
pub fn collect(world: &mut hecs::World, dt: f32, player: Vec3) -> Vec<((i32, i32), Option<String>)> {
    let mut taken = Vec::new();
    for (e, gem, look) in world.query_mut::<(hecs::Entity, &Gem, &mut Look)>() {
        look.begin_step();
        if gem.spin {
            look.yaw += dt * 1.6;
        }
        if (look.pos + Vec3::Y * 0.3 - (player + Vec3::Y * 0.9)).length() < 1.1 {
            taken.push((e, gem.cell, gem.what.clone()));
        }
    }
    taken
        .into_iter()
        .map(|(e, cell, what)| {
            let _ = world.despawn(e);
            (cell, what)
        })
        .collect()
}

/// How many things of this display name have been collected.
pub fn found_count(collected: &HashSet<(i32, i32)>, land: &Land, item_models: &HashMap<String, crate::worldload::ItemModel>, display: &str) -> usize {
    collected
        .iter()
        .filter(|&&(k, tag)| tag == -1 && k >= 0)
        .filter_map(|&(k, _)| land.items.get(k as usize))
        .filter(|it| item_models.get(&it.kind).is_some_and(|m| m.display == display))
        .count()
}
