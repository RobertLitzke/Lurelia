use crate::library::Op;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Placed, Poi, Pos};
use crate::world::noise::{hash2, perlin2, unit};
use crate::world::shape;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// A frozen waterfall: the fall from a hanging valley's lip turned to ice for
/// the winter, built as voxels down the wall so it can be seen from across
/// the fjord and climbed. Walking from `from` (on the valley floor above)
/// toward `toward` (out over the drop) finds the lip; from there the ice
/// follows the face down, metre by metre, to the sea or the foot of the wall:
/// bulges plastered on the rock `width` m across, and icicles hanging off
/// every ledge, longer where the face overhangs. `stream` (a path down the
/// valley floor to the lip) is the frozen stream that feeds it.
///
/// Needs a library structure named `icefall` (any size), which the placement
/// is filed under for names and tags.
#[derive(Deserialize, Debug)]
pub struct Icefall {
    from: Pos,
    toward: Pos,
    width: f32,
    ice: String,
    /// Bands of older, bluer ice.
    #[serde(default)]
    blue: Option<String>,
    #[serde(default)]
    stream: Vec<Pos>,
    #[serde(default)]
    poi: Option<String>,
}

impl Process for Icefall {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let Some(def) = cx.lib.structure("icefall") else {
            cx.warnings.push("icefall: the library has no structure called icefall".into());
            return "skipped".into();
        };
        let a = cx.pos(&self.from);
        let b = cx.pos(&self.toward);
        let ice_m = cx.mat(&self.ice);
        let (seed, cause) = (cx.seed, cx.cause);
        let dir = (b - a).normalize_or_zero();
        let side = Vec2::new(-dir.y, dir.x);
        let f = &mut cx.f;
        let top = f.height(a.x, a.y);
        // The lip: the first point the ground falls away from the floor.
        let total = a.distance(b);
        let mut s = 0.0;
        while s < total && f.height(a.x + dir.x * s, a.y + dir.y * s) > top - 1.5 {
            s += 0.5;
        }
        if s >= total {
            cx.warnings.push("icefall: no lip between from and toward".into());
            return "skipped".into();
        }
        // Down the face: where the wall stands at each height.
        let mut face: Vec<(f32, Vec2)> = Vec::new();
        let mut y = top - 0.5;
        while y > f.sea - 0.5 && s < total + 200.0 {
            while f.height(a.x + dir.x * s, a.y + dir.y * s) > y && s < total + 200.0 {
                s += 0.25;
            }
            face.push((y, a + dir * s));
            y -= 1.0;
        }
        // The ice, in world metres: bulges on the face and icicles off it.
        let blue = self.blue.clone().unwrap_or_else(|| self.ice.clone());
        let mut parts: Vec<(Vec3, Vec3, bool, String)> = Vec::new(); // (centre or base, radius or (r, len, _), is icicle, mat)
        let half = self.width * 0.5;
        for (k, (y, p)) in face.iter().enumerate() {
            let n = perlin2(*y * 0.15, 0.0, seed);
            let w = half * (0.6 + 0.4 * n.abs() + 0.15 * perlin2(*y * 0.6, 3.0, seed + 1));
            let off = side * (perlin2(*y * 0.05, 9.0, seed + 2) * half * 0.4);
            // Where the face steps back, the ice above hangs out over it.
            let next = face.get(k + 1).map_or(*p, |q| q.1);
            let step = next.distance(*p);
            let c = *p + off - dir * 0.3;
            let mat = if perlin2(*y * 0.35, 1.0, seed + 3) > 0.25 { blue.clone() } else { self.ice.clone() };
            parts.push((Vec3::new(c.x, *y, c.y), Vec3::new(w, 0.9, 1.0 + 0.5 * step.min(3.0)), false, mat));
            // Icicles along the edge of a ledge.
            if step > 0.6 || k % 3 == 0 {
                let count = (w * 1.2) as i32 + 2;
                for j in 0..count {
                    let h = hash2(k as i32, j, seed + 4);
                    let across = (unit(h) * 2.0 - 1.0) * w;
                    let len = 1.0 + unit(hash2(k as i32, j, seed + 5)) * (2.0 + step * 2.5).min(7.0);
                    let r = 0.2 + 0.35 * unit(hash2(k as i32, j, seed + 6));
                    let q = *p + off + side * across + dir * (0.2 + 0.5 * step.min(2.0));
                    parts.push((Vec3::new(q.x, *y - len, q.y), Vec3::new(r, len, 0.0), true, self.ice.clone()));
                }
            }
        }
        if parts.is_empty() {
            return "skipped".into();
        }
        // The cone at the foot: spray and drips frozen into a heap against
        // the wall, wider than the fall, half in the water.
        if let Some((y, p)) = face.last() {
            let base = *p + dir * 1.5;
            for k in 0..7 {
                let t = k as f32 / 6.0;
                let r = half * (1.6 - 1.2 * t);
                let c = base - dir * (t * 3.0);
                parts.push((Vec3::new(c.x, *y + t * half * 1.4, c.y), Vec3::new(r, half * 0.5, r * 0.8), false, self.ice.clone()));
            }
        }
        // The bounding box, then everything in voxels from its corner.
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for (c, r, icicle, _) in &parts {
            let (a, b) = if *icicle { (*c - Vec3::new(r.x, 0.0, r.x), *c + Vec3::new(r.x, r.y, r.x)) } else { (*c - *r, *c + *r) };
            lo = lo.min(a);
            hi = hi.max(b);
        }
        let origin = [(lo.x * 4.0).floor() as i32 - 1, (lo.y * 4.0).floor() as i32 - 1, (lo.z * 4.0).floor() as i32 - 1];
        let o = Vec3::new(origin[0] as f32, origin[1] as f32, origin[2] as f32) * 0.25;
        let size = [((hi.x - o.x) * 4.0).ceil() as i32 + 2, ((hi.y - o.y) * 4.0).ceil() as i32 + 2, ((hi.z - o.z) * 4.0).ceil() as i32 + 2];
        let ops: Vec<Op> = parts
            .iter()
            .map(|(c, r, icicle, mat)| {
                let l = (*c - o) * 4.0;
                if *icicle {
                    Op::Cylinder { base: [l.x, l.y, l.z], radius: r.x * 4.0, height: r.y * 4.0, mat: mat.clone(), part: None }
                } else {
                    Op::Ellipsoid { centre: [l.x, l.y, l.z], radius: [r.x * 4.0, r.y * 4.0, r.z * 4.0], mat: mat.clone(), part: None }
                }
            })
            .collect();
        let lib = cx.lib;
        let compiled = shape::compile(&ops, size, &|n: &str| lib.material(n));
        let (solid, size_v) = (compiled.solid, compiled.size);
        cx.shapes.push(compiled);
        cx.placed.push(Placed { shape: cx.shapes.len() - 1, def, origin, rot: 0, age: 0.0, cause, seed });
        // Nothing grows through it, and the frozen stream above the lip.
        let f = &mut cx.f;
        for (_, p) in &face {
            for t in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
                let q = *p + side * t * half;
                let (x, z) = (q.x.floor() as i32, q.y.floor() as i32);
                if x >= 0 && z >= 0 && (x as usize) < f.n && (z as usize) < f.n {
                    f.occupied[z as usize * f.n + x as usize] = true;
                }
            }
        }
        let mut frozen = 0;
        let line: Vec<Vec2> = self.stream.iter().map(|p| cx.pos(p)).collect();
        let f = &mut cx.f;
        for w in line.windows(2) {
            let len = w[0].distance(w[1]);
            let mut t = 0.0;
            while t <= len {
                let p = w[0] + (w[1] - w[0]) * (t / len.max(0.01));
                let wob = perlin2(p.x * 0.08, p.y * 0.08, seed + 7) * 2.0;
                for dx in -3..=3 {
                    for dz in -3..=3 {
                        let q = p + Vec2::new(dx as f32, dz as f32);
                        if q.distance(p + side * wob) > 1.8 {
                            continue;
                        }
                        let (x, z) = (q.x.floor() as i32, q.y.floor() as i32);
                        if x < 0 || z < 0 || (x as usize) >= f.n || (z as usize) >= f.n {
                            continue;
                        }
                        let i = z as usize * f.n + x as usize;
                        if f.surface[i] != ice_m {
                            f.h[i] -= 0.35;
                            f.surface[i] = ice_m;
                            f.occupied[i] = true;
                            frozen += 1;
                        }
                    }
                }
                t += 1.0;
            }
        }
        let lip = face[0].1;
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: lip, cause });
        }
        format!("icefall from {top:.0} m at {:.0},{:.0}, {} m down the face, {} pieces ({solid} voxels in {size_v:?} from {origin:?}), stream {frozen} m2", lip.x, lip.y, face.len(), parts.len())
    }
}
