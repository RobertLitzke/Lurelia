//! Biome classification: each cell takes the highest-priority library biome
//! whose rules match.
//!
//! Every cell is decided on its own from fields nothing here writes, so the
//! work is split across the machine's cores in bands of rows. On a 5.8 km
//! world it is 33 million cells against up to ninety biomes, which was the
//! largest single stage of a world build (about 4.9 s of 13 s).

use crate::library::Library;
use crate::world::fields::*;
use crate::world::noise::fbm2;
use glam::Vec2;

/// Pick each cell's biome: the highest-priority library biome whose rules match.
pub fn classify(f: &mut Fields, lib: &Library, default: usize, seabed: Option<usize>, allowed: &[bool]) {
    let regions: Vec<Option<u8>> = lib
        .biomes
        .iter()
        .map(|b| b.rules.region.as_ref().map(|r| f.regions.iter().position(|x| x == r).map_or(255, |k| k as u8 + 1)))
        .collect();
    let n = f.n;
    // Bands of whole rows, one per core. `biome` is taken out of the fields
    // first so the bands can be written while everything else is read.
    let threads = crate::world::fields::build_threads();
    let rows_per = n.div_ceil(threads).max(1);
    let mut warped = vec![0usize; n * n];
    let mut biome = std::mem::take(&mut f.biome);
    biome.clear();
    biome.resize(n * n, default as u8);
    let fields = &*f;
    std::thread::scope(|sc| {
        for (band, w) in warped.chunks_mut(rows_per * n).enumerate() {
            sc.spawn(move || {
                let base = band * rows_per * n;
                for (k, out) in w.iter_mut().enumerate() {
                    let i = base + k;
                    let p = fields.centre(i);
                    let v = Vec2::new(fbm2(p.x * 0.02, p.y * 0.02, 2, 0x51ed), fbm2(p.x * 0.02 + 40.0, p.y * 0.02, 2, 0x2545)) * 14.0;
                    let q = p + v;
                    // Keep the sea/land line exact: only warp within the same side.
                    let j = fields.idx(q.x, q.y);
                    *out = if (fields.h[j] < fields.sea) == (fields.h[i] < fields.sea) { j } else { i };
                }
            });
        }
    });
    let warped = &warped;
    let regions = &regions;
    let fields = &*f;
    std::thread::scope(|sc| {
        for (band, out) in biome.chunks_mut(rows_per * n).enumerate() {
            sc.spawn(move || {
                let base = band * rows_per * n;
                for (k, cell) in out.iter_mut().enumerate() {
                    let i = base + k;
                    *cell = one(fields, lib, default, seabed, allowed, regions, warped[i], i);
                }
            });
        }
    });
    f.biome = biome;
}

/// The biome for cell `i`, whose rules are read at the warped cell `j`.
#[allow(clippy::too_many_arguments)]
fn one(f: &Fields, lib: &Library, default: usize, seabed: Option<usize>, allowed: &[bool], regions: &[Option<u8>], j: usize, i: usize) -> u8 {
    let within = |r: &Option<[f32; 2]>, v: f32| r.is_none_or(|[a, b]| v >= a && v <= b);
    if let Some(&b) = f.biome_pin.get(&(i as u32)) {
        if allowed.get(b as usize).copied().unwrap_or(false) {
            return b;
        }
    }
    {
        let elev = f.h[j] - f.sea;
        let under = f.h[i] < f.sea;
        let burn = if f.burn_year[j] == i32::MIN { None } else { Some(-f.burn_year[j] as f32) };
        let mut best: Option<(i32, usize)> = None;
        for (bi, b) in lib.biomes.iter().enumerate() {
            if !allowed[bi] {
                continue;
            }
            let r = &b.rules;
            if r.underwater.is_some_and(|u| u != under) {
                continue;
            }
            if r.underwater.is_none() && under && !r.amphibious {
                continue;
            }
            if !(within(&r.elev, elev)
                && within(&r.slope, f.slope[j])
                && within(&r.moist, f.moist[j])
                && within(&r.coast, f.coast_dist[j])
                && within(&r.river, f.river_dist[j])
                && within(&r.strange, f.strange[j]))
            {
                continue;
            }
            if let Some(range) = &r.burn_age {
                match burn {
                    Some(a) if a >= range[0] && a <= range[1] => {}
                    _ => continue,
                }
            }
            if let Some(rid) = regions[bi] {
                if f.region[if r.crisp { i } else { j }] != rid {
                    continue;
                }
            }
            if best.is_none_or(|(p, _)| b.priority > p) {
                best = Some((b.priority, bi));
            }
        }
        match best {
            Some((_, b)) => b as u8,
            None if under => seabed.unwrap_or(default) as u8,
            None => default as u8,
        }
    }
}
