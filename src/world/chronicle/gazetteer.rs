//! The gazetteer: a plain-text account of a built world for its author.

use crate::world::fields::*;

/// A plain-text description of the built world for an author: what each event
/// did, the notable places, and where the biomes ended up.
pub fn gazetteer(w: &World) -> String {
    let f = &w.f;
    let mut out = String::new();
    out.push_str(&format!(
        "World {} ({:.0} m, sea {:.0} m), built in {:.0} ms\n\n",
        w.def.name, w.def.size_m, w.def.sea_level_m, w.build_ms
    ));
    out.push_str("Events:\n");
    for l in &w.log {
        out.push_str(&format!("  {l}\n"));
    }
    out.push_str("\nPoints of interest:\n");
    for p in &w.pois {
        out.push_str(&format!("  {:<28} {:>5.0},{:<5.0} h {:.1}\n", p.name, p.pos.x, p.pos.y, f.height(p.pos.x, p.pos.y)));
    }
    if !w.areas.is_empty() {
        out.push_str(
            "
Named areas (centre, size, how far in before the HUD names it):
",
        );
        let mut ks: Vec<usize> = (0..w.areas.len()).collect();
        ks.sort_by(|a, b| w.areas[*b].area_m2.partial_cmp(&w.areas[*a].area_m2).unwrap());
        for k in ks {
            let a = &w.areas[k];
            let c = (a.lo + a.hi) * 0.5;
            out.push_str(&format!(
                "  {:<28} {:>5.0},{:<5.0} {:>6.1} ha  {:>4.0} m  ({})
",
                a.name,
                c.x,
                c.y,
                a.area_m2 / 10000.0,
                a.inside_m,
                a.kind
            ));
        }
    }
    if !w.signs.is_empty() {
        out.push_str(
            "
Signs:
",
        );
        for sg in &w.signs {
            out.push_str(&format!(
                "  {:>5.0},{:<5.0} {:>4.0} deg  {:<16} {}
",
                sg.at.x,
                sg.at.z,
                sg.bearing,
                w.lib.signs[sg.style].name,
                sg.text.join(" / ")
            ));
        }
    }
    out.push_str(
        "
Built structures (centre, age):
",
    );
    for pl in &w.placed {
        let d = &w.lib.structures[pl.def];
        if !d.decays && !d.tags.iter().any(|t| t == "anomaly") {
            continue;
        }
        let (x0, z0, x1, z1) = pl.footprint(&w.shapes);
        out.push_str(&format!(
            "  {:<22} {:>5.0},{:<5.0} {:.0} y
",
            d.name,
            (x0 + x1) as f32 * 0.125,
            (z0 + z1) as f32 * 0.125,
            pl.age
        ));
    }
    // Natural features (rocks, logs, beds a biome scatters), which the list
    // above leaves out: how many of each and where the first few stand, so
    // tools/shoot_structure.py can find one to photograph.
    let mut natural: Vec<(&str, Vec<(f32, f32)>)> = Vec::new();
    for pl in &w.placed {
        let d = &w.lib.structures[pl.def];
        if d.decays || d.tags.iter().any(|t| t == "anomaly") {
            continue;
        }
        let (x0, z0, x1, z1) = pl.footprint(&w.shapes);
        let at = ((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125);
        match natural.iter_mut().find(|(n, _)| *n == d.name.as_str()) {
            Some((_, list)) => list.push(at),
            None => natural.push((d.name.as_str(), vec![at])),
        }
    }
    natural.sort_by(|a, b| a.0.cmp(b.0));
    out.push_str("\nNatural features (count, first centres):\n");
    for (name, list) in &natural {
        let first: Vec<String> = list.iter().take(6).map(|(x, z)| format!("{x:.0},{z:.0}")).collect();
        out.push_str(&format!("  {:<22} {:>5}  {}\n", name, list.len(), first.join("  ")));
    }
    // Peaks: local maxima over 60 m windows.
    out.push_str("\nPeaks:\n");
    let step = 20usize;
    let n = f.n;
    let mut peaks = Vec::new();
    for z in (0..n).step_by(step) {
        for x in (0..n).step_by(step) {
            let i = z * n + x;
            if f.h[i] < f.sea + 10.0 {
                continue;
            }
            let mut top = true;
            for dz in -3i32..=3 {
                for dx in -3i32..=3 {
                    let (xx, zz) = (x as i32 + dx * step as i32, z as i32 + dz * step as i32);
                    if xx >= 0 && zz >= 0 && (xx as usize) < n && (zz as usize) < n && f.h[zz as usize * n + xx as usize] > f.h[i] {
                        top = false;
                    }
                }
            }
            if top {
                peaks.push((f.h[i], f.centre(i)));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    for (h, p) in peaks.iter().take(10) {
        out.push_str(&format!("  {:>5.0},{:<5.0} h {:.1}\n", p.x, p.y, h));
    }
    // A coarse height table, so an author can place things by elevation.
    let step_m = 64.0;
    let cols = (w.def.size_m / step_m) as usize;
    out.push_str(&format!("\nGround height (m, sea {:.0}) every {step_m:.0} m; rows z, columns x:\n      ", f.sea));
    for cx in 0..cols {
        out.push_str(&format!("{:>5}", (cx as f32 * step_m + step_m * 0.5) as i32));
    }
    out.push('\n');
    for cz in 0..cols {
        let z = cz as f32 * step_m + step_m * 0.5;
        out.push_str(&format!("{:>5} ", z as i32));
        for cx in 0..cols {
            let x = cx as f32 * step_m + step_m * 0.5;
            let h = f.height(x, z);
            if h < f.sea {
                out.push_str("    ~");
            } else {
                out.push_str(&format!("{:>5.0}", h));
            }
        }
        out.push('\n');
    }
    // A close-up height grid round a point, for placing things against a wall
    // or a rim: VG_PROBE=x,z (optionally ,step) when running --gazetteer.
    if let Ok(v) = std::env::var("VG_PROBE") {
        let n: Vec<f32> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        if n.len() >= 2 {
            let st = n.get(2).copied().unwrap_or(2.0);
            out.push_str(&format!("\nProbe round {:.0},{:.0}, every {st} m (r = river, * = occupied); rows z, columns x:\n      ", n[0], n[1]));
            for k in -8..=8 {
                out.push_str(&format!("{:>6}", (n[0] + k as f32 * st) as i32));
            }
            out.push('\n');
            for j in -8..=8 {
                let z = n[1] + j as f32 * st;
                out.push_str(&format!("{:>5} ", z as i32));
                for k in -8..=8 {
                    let x = n[0] + k as f32 * st;
                    let i = f.idx(x, z);
                    let tag = if f.water[i] == 1 {
                        'r'
                    } else if f.occupied[i] {
                        '*'
                    } else {
                        ' '
                    };
                    out.push_str(&format!("{:>5.1}{tag}", f.height(x, z)));
                }
                out.push('\n');
            }
        }
    }
    out.push_str("\nBiomes (share of land):\n");
    let mut counts = vec![0usize; w.lib.biomes.len()];
    let mut land = 0;
    for i in 0..n * n {
        if f.h[i] >= f.sea {
            counts[f.biome[i] as usize] += 1;
            land += 1;
        }
    }
    for (b, c) in counts.iter().enumerate() {
        if *c > 0 {
            out.push_str(&format!("  {:<22} {:>5.1}%\n", w.lib.biomes[b].name, *c as f32 * 100.0 / land.max(1) as f32));
        }
    }
    // The sea over the land: how much of the world is under it, by depth, and
    // which biomes are (shallows a walker wades, a kayak floats on).
    let bands = [0.5f32, 1.0, 1.5, 2.0, 3.0, f32::MAX];
    let mut by_depth = [0usize; 6];
    let mut wet = vec![(0usize, 0.0f32); w.lib.biomes.len()];
    for i in 0..n * n {
        let d = f.sea - f.h[i];
        if d > 0.0 {
            by_depth[bands.iter().position(|b| d < *b).unwrap_or(5)] += 1;
            let e = &mut wet[f.biome[i] as usize];
            e.0 += 1;
            e.1 += d;
        }
    }
    let total: usize = by_depth.iter().sum();
    out.push_str(&format!("\nUnder the sea: {:.1}% of the world; by depth", total as f32 * 100.0 / (n * n) as f32));
    for (k, c) in by_depth.iter().enumerate() {
        let lo = if k == 0 { 0.0 } else { bands[k - 1] };
        let label = if k == 5 { format!(">{lo}") } else { format!("{lo}-{}", bands[k]) };
        out.push_str(&format!("  {label} m {:.0}%", *c as f32 * 100.0 / total.max(1) as f32));
    }
    out.push('\n');
    for (b, (c, s)) in wet.iter().enumerate() {
        if *c > 0 {
            out.push_str(&format!(
                "  {:<22} {:>5.1}% of the water, mean depth {:.2} m\n",
                w.lib.biomes[b].name,
                *c as f32 * 100.0 / total.max(1) as f32,
                s / *c as f32
            ));
        }
    }
    // Plants by species, most first.
    let mut per: std::collections::BTreeMap<u16, usize> = std::collections::BTreeMap::new();
    for t in &w.trees {
        *per.entry(t.plant).or_default() += 1;
    }
    let mut per: Vec<(u16, usize)> = per.into_iter().collect();
    per.sort_by(|a, b| b.1.cmp(&a.1));
    out.push_str("\nPlants:");
    for (p, n) in per {
        let name = w.lib.plants.get(p as usize).map_or("(other)", |d| d.name.as_str());
        out.push_str(&format!(" {name} {n},"));
    }
    out.push('\n');
    out.push_str(&format!(
        "\n{} structures placed, {} trees, {} items. Spawn {:.0},{:.0}.\n",
        w.placed.len(),
        w.trees.len(),
        w.items.len(),
        w.spawn.x,
        w.spawn.z
    ));
    if !w.warnings.is_empty() {
        out.push_str("\nWarnings:\n");
        for x in &w.warnings {
            out.push_str(&format!("  {x}\n"));
        }
    }
    out
}
