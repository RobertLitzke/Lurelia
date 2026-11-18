//! The land's own horizon, baked from the world heightmap when a world loads.
//!
//! The shadow cascades reach 160 m. Past that nothing was shadowed at all, so
//! a far hillside showed the same value on the side facing the sun and the
//! side away from it and the whole distance read as flat paper. This fills
//! that in without geometry: for every cell of a coarse grid over the world,
//! the elevation of the skyline in each of `AZIMUTHS` compass directions. A
//! point is in the land's shadow when the sun stands lower than the skyline
//! in the sun's own direction, which is one texture read at any distance, at
//! any hour, out to the world's edge.
//!
//! The same scan gives how much sky each cell can see, which is the large
//! scale ambient occlusion the per-voxel one cannot reach: hollows and valley
//! floors sit in less light than the ridges above them.
//!
//! Layers are the azimuths in order, then one more holding the sky openness.
//! Each is one byte: the sine of the skyline's elevation, and openness as a
//! fraction. Azimuth k points along (sin, cos) of `k * 2pi / AZIMUTHS` in
//! (x, z), so the shader finds its bin with `atan2(sun.x, sun.z)`.

/// Side of the baked map. 256 over a 1.5 km world is about 6 m a cell, which
/// is the scale of the relief that reads from half a kilometre away; the
/// cascades do everything finer than that, nearer than that.
pub const HORIZON_N: u32 = 256;
pub const AZIMUTHS: usize = 16;
/// One layer per azimuth, then the sky openness.
pub const LAYERS: u32 = AZIMUTHS as u32 + 1;

/// How far a skyline may stand from a cell. Beyond a couple of kilometres a
/// ridge is below the horizon of the earth's own curve anyway.
const RANGE_M: f32 = 2000.0;

/// A flat world: nothing on the skyline, all the sky visible.
pub fn blank() -> Vec<u8> {
    let n = (HORIZON_N * HORIZON_N) as usize;
    let mut v = vec![0u8; n * LAYERS as usize];
    v[n * AZIMUTHS..].fill(255);
    v
}

/// Bake the map from `heights`, an `n` x `n` grid of ground heights in metres
/// over a world `world_size` metres across (what `Renderer::set_world` is
/// given). Returns `LAYERS * HORIZON_N * HORIZON_N` bytes, layer by layer.
pub fn bake(heights: &[f32], n: usize, world_size: f32) -> Vec<u8> {
    assert_eq!(heights.len(), n * n, "heightmap must be square");
    let side = HORIZON_N as usize;
    let cell = world_size / side as f32;
    let hcell = world_size / n as f32;
    // Height at a world position, bilinear, clamped outside the world.
    let at = |x: f32, z: f32| -> f32 {
        let (u, v) = (x / hcell - 0.5, z / hcell - 0.5);
        let (i, j) = (u.floor(), v.floor());
        let (fu, fv) = (u - i, v - j);
        let ix = (i as i32).clamp(0, n as i32 - 1) as usize;
        let iz = (j as i32).clamp(0, n as i32 - 1) as usize;
        let ix1 = (ix + 1).min(n - 1);
        let iz1 = (iz + 1).min(n - 1);
        let a = heights[iz * n + ix] + (heights[iz * n + ix1] - heights[iz * n + ix]) * fu;
        let b = heights[iz1 * n + ix] + (heights[iz1 * n + ix1] - heights[iz1 * n + ix]) * fu;
        a + (b - a) * fv
    };
    // The distances stepped along a ray: one heightmap cell, then widening,
    // so a near ridge is caught finely and a far one cheaply.
    let steps: Vec<f32> = {
        let mut v = Vec::new();
        let mut r = hcell;
        while r < RANGE_M.min(world_size * 1.5) {
            v.push(r);
            r = r * 1.3 + hcell * 0.25;
        }
        v
    };
    let dirs: [(f32, f32); AZIMUTHS] = std::array::from_fn(|k| {
        let a = k as f32 * std::f32::consts::TAU / AZIMUTHS as f32;
        (a.sin(), a.cos())
    });

    let plane = side * side;
    let mut out = vec![0u8; plane * LAYERS as usize];
    let threads = std::thread::available_parallelism().map(|t| t.get()).unwrap_or(4).min(8);
    let rows = side.div_ceil(threads);
    // Each worker fills whole rows of every layer, so the layers are written
    // through one slice per row rather than one per thread.
    let mut band: Vec<Vec<u8>> = Vec::new();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for t in 0..threads {
            let z0 = t * rows;
            let z1 = ((t + 1) * rows).min(side);
            let (at, steps, dirs) = (&at, &steps, &dirs);
            handles.push(s.spawn(move || {
                let mut buf = vec![0u8; (z1.saturating_sub(z0)) * side * LAYERS as usize];
                let rows_here = z1.saturating_sub(z0);
                for iz in z0..z1 {
                    for ix in 0..side {
                        let x0 = (ix as f32 + 0.5) * cell;
                        let z0m = (iz as f32 + 0.5) * cell;
                        let h0 = at(x0, z0m);
                        let mut open = 0.0f32;
                        for (k, &(dx, dz)) in dirs.iter().enumerate() {
                            let mut sin_max = 0.0f32;
                            for &r in steps.iter() {
                                let dh = at(x0 + dx * r, z0m + dz * r) - h0;
                                if dh <= 0.0 {
                                    continue;
                                }
                                let s = dh / (r * r + dh * dh).sqrt();
                                if s > sin_max {
                                    sin_max = s;
                                }
                            }
                            // Sky this way, weighted by the cosine a flat
                            // surface sees it at: 1 - sin^2 of the skyline.
                            open += 1.0 - sin_max * sin_max;
                            let row = (iz - z0) * side + ix;
                            buf[k * rows_here * side + row] = (sin_max * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                        let row = (iz - z0) * side + ix;
                        buf[AZIMUTHS * rows_here * side + row] = ((open / AZIMUTHS as f32) * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                }
                buf
            }));
        }
        for h in handles {
            band.push(h.join().unwrap());
        }
    });
    for (t, buf) in band.into_iter().enumerate() {
        let z0 = t * rows;
        let z1 = ((t + 1) * rows).min(side);
        let rows_here = z1.saturating_sub(z0);
        if rows_here == 0 {
            continue;
        }
        for l in 0..LAYERS as usize {
            let src = &buf[l * rows_here * side..(l + 1) * rows_here * side];
            out[l * plane + z0 * side..l * plane + z1 * side].copy_from_slice(src);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cone in the middle of a flat world: cells beside it see it on their
    /// skyline and less of the sky, cells in the corner see neither.
    #[test]
    fn a_hill_stands_on_the_skyline() {
        let n = 128;
        let size = 512.0f32;
        let cell = size / n as f32;
        let mut h = vec![0.0f32; n * n];
        for z in 0..n {
            for x in 0..n {
                let (dx, dz) = ((x as f32 + 0.5) * cell - 256.0, (z as f32 + 0.5) * cell - 256.0);
                let d = (dx * dx + dz * dz).sqrt();
                h[z * n + x] = (60.0 - d * 0.75).max(0.0);
            }
        }
        let map = bake(&h, n, size);
        let side = HORIZON_N as usize;
        let plane = side * side;
        let idx = |x: usize, z: usize| z * side + x;
        // A cell due south of the hill (larger z), looking north (-Z) at it.
        // Azimuth 8 is (sin, cos) of pi: straight along -Z.
        let (hx, hz) = (side / 2, side * 3 / 4);
        assert!(map[8 * plane + idx(hx, hz)] > 40, "the hill should stand on the skyline to the north");
        assert!(map[idx(hx, hz)] < 10, "and nothing on it to the south");
        // The corner is far from the hill and sees nearly all of the sky.
        assert!(map[AZIMUTHS * plane + idx(2, 2)] > 230, "open ground sees the sky");
        assert!(map[AZIMUTHS * plane + idx(hx, hz)] < map[AZIMUTHS * plane + idx(2, 2)], "ground under the hill sees less of it");
    }

    #[test]
    fn blank_is_open() {
        let v = blank();
        let plane = (HORIZON_N * HORIZON_N) as usize;
        assert_eq!(v.len(), plane * LAYERS as usize);
        assert_eq!(v[0], 0);
        assert_eq!(v[AZIMUTHS * plane], 255);
    }
}
