//! A top-down map of a chronicle world for authors: biomes in their map colours
//! with hill shading, rivers and old beds, paths, structures, buried items and
//! the spawn point. `--map out.png` writes it.

use super::fields::World;
use glam::Vec2;

pub fn write(w: &World, path: &std::path::Path, px: usize) -> std::io::Result<()> {
    let f = &w.f;
    let scale = f.size / px as f32;
    let mut img = vec![0u8; px * px * 4];
    for y in 0..px {
        for x in 0..px {
            let (wx, wz) = ((x as f32 + 0.5) * scale, (y as f32 + 0.5) * scale);
            let i = f.idx(wx, wz);
            let h = f.h[i];
            let hx = f.height(wx + scale, wz);
            let hz = f.height(wx, wz + scale);
            let shade = (1.0 + ((h - hx) + (h - hz)) * 0.5 / scale.max(0.5)).clamp(0.55, 1.35);
            let b = &w.lib.biomes[f.biome[i] as usize];
            let mut c = b.map_color.unwrap_or([128.0, 128.0, 128.0]);
            if h < f.sea {
                let d = ((f.sea - h) / 12.0).min(1.0);
                c = [40.0 + 50.0 * (1.0 - d), 90.0 + 60.0 * (1.0 - d), 140.0 + 40.0 * (1.0 - d)];
            }
            match f.water[i] {
                1 => c = [70.0, 130.0, 190.0],
                2 => c = [170.0, 150.0, 120.0],
                _ => {}
            }
            if f.water[i] == 0 && f.surface[i] != 0 && f.occupied[i] && h > f.sea {
                c = [200.0, 180.0, 140.0];
            }
            // Strangeness tints toward violet.
            let s = f.strange[i] * 0.35;
            c = [c[0] + (190.0 - c[0]) * s, c[1] + (150.0 - c[1]) * s, c[2] + (230.0 - c[2]) * s];
            let k = (y * px + x) * 4;
            for j in 0..3 {
                img[k + j] = (c[j] * if h < f.sea { 1.0 } else { shade }).clamp(0.0, 255.0) as u8;
            }
            img[k + 3] = 255;
        }
    }
    let mut dot = |p: Vec2, r: i32, c: [u8; 3]| {
        let (cx, cy) = ((p.x / scale) as i32, (p.y / scale) as i32);
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && (x as usize) < px && (y as usize) < px && dx * dx + dy * dy <= r * r {
                    let k = (y as usize * px + x as usize) * 4;
                    img[k..k + 3].copy_from_slice(&c);
                }
            }
        }
    };
    for t in w.trees.iter().step_by(3) {
        dot(Vec2::new(t.pos.x, t.pos.z), 0, [30, 70, 35]);
    }
    for p in &w.placed {
        let (x0, z0, x1, z1) = p.footprint(&w.shapes);
        let c = Vec2::new((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125);
        let natural = !w.lib.structures[p.def].decays;
        dot(c, if natural { 0 } else { 2 }, if natural { [120, 120, 120] } else { [150, 40, 30] });
    }
    for it in &w.items {
        dot(Vec2::new(it.pos.x, it.pos.z), 1, [120, 255, 255]);
    }
    dot(Vec2::new(w.spawn.x, w.spawn.z), 4, [255, 255, 0]);
    crate::util::save_png(path, px as u32, px as u32, &img)
}
