//! Shape programs: the small language library structures and creature models are
//! written in (boxes, walls, gable roofs, ellipsoids, cylinders, logs, discs,
//! repeats). A program is compiled once into a dense voxel array of (material,
//! part); placing a structure then only rotates, looks up and decays.

use crate::library::Op;

/// What a voxel is part of, for decay: roofs go first, stone lasts.
pub const PART_NONE: u8 = 0;
pub const PART_ROOF: u8 = 1;
pub const PART_TIMBER: u8 = 2;
pub const PART_WALL: u8 = 3;
pub const PART_STONE: u8 = 4;
pub const PART_FLOOR: u8 = 5;
pub const PART_FRAME: u8 = 6;
pub const PART_GLASS: u8 = 7;
pub const PART_METAL: u8 = 8;
pub const PART_CLOTH: u8 = 9;

/// The material id a structure op names as `"carve"`: when the structure is
/// stamped it removes whatever is there, terrain included (alcoves, adits,
/// cellars), where `"air"` only leaves the terrain alone.
pub const CARVE: u8 = 255;

pub fn part_code(p: Option<&String>) -> u8 {
    match p.map(|s| s.as_str()) {
        Some("roof") => PART_ROOF,
        Some("timber") => PART_TIMBER,
        Some("wall") => PART_WALL,
        Some("stone") => PART_STONE,
        Some("floor") => PART_FLOOR,
        Some("frame") => PART_FRAME,
        Some("glass") => PART_GLASS,
        Some("metal") => PART_METAL,
        Some("cloth") => PART_CLOTH,
        _ => PART_NONE,
    }
}

/// How fast each part decays: the fraction gone after a long time.
pub fn decay_rate(part: u8) -> f32 {
    match part {
        PART_ROOF => 0.95,
        PART_CLOTH => 0.98,
        PART_TIMBER => 0.7,
        PART_FRAME => 0.55,
        PART_GLASS => 0.9,
        PART_FLOOR => 0.25,
        PART_WALL => 0.3,
        PART_METAL => 0.2,
        PART_STONE => 0.12,
        _ => 0.0,
    }
}

/// A compiled shape: `size` voxels, x fastest, then z, then y.
#[derive(Clone, Default)]
pub struct Shape {
    pub size: [i32; 3],
    pub mat: Vec<u8>,
    pub part: Vec<u8>,
    /// Number of solid voxels, and the highest solid voxel + 1.
    pub solid: usize,
    pub top: i32,
    /// Per (x, z) column, the roof material over it (0 if none), for the
    /// rubble a collapsed roof leaves on the floor.
    pub roof: Vec<u8>,
    /// Per (x, z) column, the height of its highest non-roof voxel + 1: walls
    /// crumble down from here.
    pub col_top: Vec<i32>,
    /// The `segment` ops' pieces, in local voxel units: drawn by the mesher
    /// as tubes along their lines where the structure stands (threads).
    pub threads: Vec<ShapeThread>,
    /// The voxels (indices into `mat`) a `segment` drew and no later op
    /// replaced: the only ones its tube may hide. A segment's thread hides
    /// the voxels it covers, so without this a steel brace hid the steel
    /// pole it was joined to, and an arc's ends punched holes in a dome.
    pub thread_vox: Vec<u32>,
    /// While compiling: `thread_vox` as one flag per voxel (empty after).
    pub(crate) owned: Vec<bool>,
}

/// One straight piece of a `segment` op.
#[derive(Clone, Copy, Debug)]
pub struct ShapeThread {
    pub a: [f32; 3],
    pub b: [f32; 3],
    /// Its radius and the radius its voxels were drawn to, voxels.
    pub r: f32,
    pub hull: f32,
    pub mat: u8,
}

impl Shape {
    pub fn idx(&self, x: i32, y: i32, z: i32) -> Option<usize> {
        let [sx, sy, sz] = self.size;
        if x < 0 || y < 0 || z < 0 || x >= sx || y >= sy || z >= sz {
            return None;
        }
        Some(((y * sz + z) * sx + x) as usize)
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> (u8, u8) {
        self.idx(x, y, z).map_or((0, 0), |i| (self.mat[i], self.part[i]))
    }
}

/// `resolve` maps a material, slot or colour name to an id (0 carves);
/// unknown names give None and are skipped.
/// The horizontal direction (x, z) of a log or disc axis ("x" or "z")
/// turned `yaw` degrees from +x toward +z.
fn turned_axis(axis: &str, yaw: f32) -> [f32; 2] {
    let a = yaw.to_radians() + if axis == "x" { 0.0 } else { std::f32::consts::FRAC_PI_2 };
    [a.cos(), a.sin()]
}

/// A structure made only of logs and discs (driftwood, bone shards) turned
/// `yaw` degrees about its anchor, for a feature that may lie at any
/// bearing rather than only at the four quarter turns. Returns the turned
/// ops with the new size and anchor, sized to just hold them; None if the
/// structure has any other op.
pub fn turn_logs(ops: &[Op], size: [i32; 3], anchor: [i32; 3], yaw: f32) -> Option<(Vec<Op>, [i32; 3], [i32; 3])> {
    let (sn, cs) = yaw.to_radians().sin_cos();
    let (ax, az) = (anchor[0] as f32, anchor[2] as f32);
    let spin = |p: [f32; 3]| {
        let (u, v) = (p[0] - ax, p[2] - az);
        [u * cs - v * sn + ax, p[1], u * sn + v * cs + az]
    };
    let mut out = Vec::with_capacity(ops.len());
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    let mut grow = |p: [f32; 3], r: f32| {
        lo = [lo[0].min(p[0] - r), lo[1].min(p[2] - r)];
        hi = [hi[0].max(p[0] + r), hi[1].max(p[2] + r)];
    };
    for op in ops {
        match op.clone() {
            Op::Log {
                base,
                radius,
                length,
                axis,
                mat,
                part,
                yaw: y0,
            } => {
                let b = spin(base);
                let d = turned_axis(&axis, y0 + yaw);
                grow(b, radius + 1.0);
                grow([b[0] + d[0] * length, 0.0, b[2] + d[1] * length], radius + 1.0);
                out.push(Op::Log {
                    base: b,
                    radius,
                    length,
                    axis,
                    mat,
                    part,
                    yaw: y0 + yaw,
                });
            }
            Op::Disc {
                centre,
                radius,
                thick,
                axis,
                mat,
                part,
                yaw: y0,
            } => {
                let c = spin(centre);
                grow(c, radius + thick + 1.0);
                out.push(Op::Disc {
                    centre: c,
                    radius,
                    thick,
                    axis,
                    mat,
                    part,
                    yaw: y0 + yaw,
                });
            }
            _ => return None,
        }
    }
    if out.is_empty() {
        return None;
    }
    // Shift everything so the turned bounds start at voxel 0.
    let (sx, sz) = (lo[0].floor(), lo[1].floor());
    for op in &mut out {
        if let Op::Log { base: p, .. } | Op::Disc { centre: p, .. } = op {
            p[0] -= sx;
            p[2] -= sz;
        }
    }
    let new_size = [(hi[0] - sx).ceil() as i32 + 1, size[1], (hi[1] - sz).ceil() as i32 + 1];
    let new_anchor = [(ax - sx).round() as i32, anchor[1], (az - sz).round() as i32];
    Some((out, new_size, new_anchor))
}

pub fn compile(ops: &[Op], size: [i32; 3], resolve: &dyn Fn(&str) -> Option<u8>) -> Shape {
    let n = (size[0] * size[1] * size[2]).max(0) as usize;
    let mut s = Shape {
        size,
        mat: vec![0; n],
        part: vec![0; n],
        solid: 0,
        top: 0,
        roof: vec![0; (size[0] * size[2]).max(0) as usize],
        col_top: vec![0; (size[0] * size[2]).max(0) as usize],
        threads: Vec::new(),
        thread_vox: Vec::new(),
        owned: Vec::new(),
    };
    run(&mut s, ops, [0, 0, 0], resolve);
    s.thread_vox = s.owned.iter().enumerate().filter(|&(i, &o)| o && s.mat[i] != 0).map(|(i, _)| i as u32).collect();
    s.owned = Vec::new();
    for z in 0..size[2] {
        for x in 0..size[0] {
            for y in (0..size[1]).rev() {
                let (m, p) = s.get(x, y, z);
                if m != 0 && p == PART_ROOF {
                    s.roof[(z * size[0] + x) as usize] = m;
                    break;
                }
            }
            for y in (0..size[1]).rev() {
                let (m, p) = s.get(x, y, z);
                if m != 0 && m != CARVE && p != PART_ROOF && p != PART_CLOTH {
                    s.col_top[(z * size[0] + x) as usize] = y + 1;
                    break;
                }
            }
        }
    }
    s.solid = s.mat.iter().filter(|&&m| m != 0).count();
    for y in (0..size[1]).rev() {
        let row = (y * size[2] * size[0]) as usize..((y + 1) * size[2] * size[0]) as usize;
        if s.mat[row].iter().any(|&m| m != 0) {
            s.top = y + 1;
            break;
        }
    }
    s
}

fn put(s: &mut Shape, x: i32, y: i32, z: i32, m: u8, p: u8) {
    if let Some(i) = s.idx(x, y, z) {
        if let Some(o) = s.owned.get_mut(i) {
            *o = false;
        }
        s.mat[i] = m;
        s.part[i] = if m == 0 { 0 } else { p };
    }
}

fn run(s: &mut Shape, ops: &[Op], off: [i32; 3], resolve: &dyn Fn(&str) -> Option<u8>) {
    let add = |a: [i32; 3]| [a[0] + off[0], a[1] + off[1], a[2] + off[2]];
    let addf = |a: [f32; 3]| [a[0] + off[0] as f32, a[1] + off[1] as f32, a[2] + off[2] as f32];
    for op in ops {
        match op {
            Op::Box { from, to, mat, part } => {
                let Some(m) = resolve(mat) else { continue };
                let (a, b) = (add(*from), add(*to));
                let p = part_code(part.as_ref());
                for y in a[1]..b[1] {
                    for z in a[2]..b[2] {
                        for x in a[0]..b[0] {
                            put(s, x, y, z, m, p);
                        }
                    }
                }
            }
            Op::Walls { from, to, mat, thick, part } => {
                let Some(m) = resolve(mat) else { continue };
                let (a, b) = (add(*from), add(*to));
                let p = part_code(part.as_ref());
                let t = (*thick).max(1);
                for y in a[1]..b[1] {
                    for z in a[2]..b[2] {
                        for x in a[0]..b[0] {
                            let inside = x >= a[0] + t && x < b[0] - t && z >= a[2] + t && z < b[2] - t;
                            if !inside {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Gable { from, to, axis, mat, rise, part } => {
                let Some(m) = resolve(mat) else { continue };
                let (a, b) = (add(*from), add(*to));
                let p = part_code(part.as_ref());
                let rise = (*rise).max(1);
                let along_x = axis == "x";
                // Slope runs across the ridge axis.
                let (c0, c1) = if along_x { (a[2], b[2]) } else { (a[0], b[0]) };
                let (l0, l1) = if along_x { (a[0], b[0]) } else { (a[2], b[2]) };
                let mut k = 0;
                loop {
                    let inset = k / rise;
                    let (lo, hi) = (c0 + inset, c1 - inset);
                    if lo >= hi {
                        break;
                    }
                    let next = (k + 1) / rise;
                    // At least the outer course on every layer: with rise 2 two
                    // layers share an inset, and the lower one had no surface,
                    // which left the roof striped with gaps.
                    let (nlo, nhi) = ((c0 + next).max(lo + 1), (c1 - next).min(hi - 1));
                    let y = a[1] + k;
                    for c in lo..hi {
                        for l in l0..l1 {
                            // The surface of each step plus the gable ends.
                            let exposed = c < nlo || c >= nhi || l == l0 || l == l1 - 1;
                            if exposed {
                                let (x, z) = if along_x { (l, c) } else { (c, l) };
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                    k += 1;
                }
            }
            Op::Ellipsoid { centre, radius, mat, part } => {
                let Some(m) = resolve(mat) else { continue };
                let c = addf(*centre);
                let p = part_code(part.as_ref());
                let r = *radius;
                for y in (c[1] - r[1]).floor() as i32..=(c[1] + r[1]).ceil() as i32 {
                    for z in (c[2] - r[2]).floor() as i32..=(c[2] + r[2]).ceil() as i32 {
                        for x in (c[0] - r[0]).floor() as i32..=(c[0] + r[0]).ceil() as i32 {
                            let d = ((x as f32 + 0.5 - c[0]) / r[0]).powi(2) + ((y as f32 + 0.5 - c[1]) / r[1]).powi(2) + ((z as f32 + 0.5 - c[2]) / r[2]).powi(2);
                            if d <= 1.0 {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Dish {
                vertex,
                radius,
                depth,
                thick,
                tilt,
                yaw,
                mat,
                part,
            } => {
                let Some(m) = resolve(mat) else { continue };
                let v = addf(*vertex);
                let p = part_code(part.as_ref());
                let (t, w) = (tilt.to_radians(), yaw.to_radians());
                let axis = [t.sin() * w.cos(), t.cos(), t.sin() * w.sin()];
                let (r, d, half) = (radius.max(1.0), *depth, thick.max(0.5) * 0.5);
                let reach = (r * r + d * d).sqrt() + thick + 1.0;
                for y in (v[1] - reach).floor() as i32..=(v[1] + reach).ceil() as i32 {
                    for z in (v[2] - reach).floor() as i32..=(v[2] + reach).ceil() as i32 {
                        for x in (v[0] - reach).floor() as i32..=(v[0] + reach).ceil() as i32 {
                            let q = [x as f32 + 0.5 - v[0], y as f32 + 0.5 - v[1], z as f32 + 0.5 - v[2]];
                            let h = q[0] * axis[0] + q[1] * axis[1] + q[2] * axis[2];
                            let rr = ((q[0] - h * axis[0]).powi(2) + (q[1] - h * axis[1]).powi(2) + (q[2] - h * axis[2]).powi(2)).sqrt();
                            if rr > r {
                                continue;
                            }
                            // Distance from the surface h = d (rr / r)^2,
                            // measured across it rather than along the axis
                            // so the shell keeps its thickness up the rim.
                            let slope = 2.0 * d * rr / (r * r);
                            if (h - d * (rr / r).powi(2)).abs() / (1.0 + slope * slope).sqrt() <= half {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Cylinder { base, radius, height, mat, part } => {
                let Some(m) = resolve(mat) else { continue };
                let b = addf(*base);
                let p = part_code(part.as_ref());
                let r = *radius;
                for y in b[1].floor() as i32..(b[1] + height).ceil() as i32 {
                    for z in (b[2] - r).floor() as i32..=(b[2] + r).ceil() as i32 {
                        for x in (b[0] - r).floor() as i32..=(b[0] + r).ceil() as i32 {
                            let d = (x as f32 + 0.5 - b[0]).powi(2) + (z as f32 + 0.5 - b[2]).powi(2);
                            if d <= r * r {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                    // A cylinder thinner than a voxel holds no voxel centre
                    // when its axis is on a voxel corner, and drew nothing:
                    // posts, knees and icicles went missing. It always fills
                    // the voxel its axis runs through.
                    if r > 0.0 && r < 0.75 {
                        put(s, b[0].floor() as i32, y, b[2].floor() as i32, m, p);
                    }
                }
            }
            Op::Log {
                base,
                radius,
                length,
                axis,
                mat,
                part,
                yaw,
            } if *yaw != 0.0 => {
                let Some(m) = resolve(mat) else { continue };
                let d = turned_axis(axis, *yaw);
                let m = crate::world::matlib::grained(m, crate::world::matlib::Axis::of(glam::Vec3::new(d[0], 0.0, d[1])));
                let b = addf(*base);
                let p = part_code(part.as_ref());
                let r = *radius;
                // The same log as the square one below: voxel centres from 0
                // to `length` along it, within `r` of its centre line across.
                let e = [b[0] + d[0] * length, b[2] + d[1] * length];
                let (x0, x1) = ((b[0].min(e[0]) - r).floor() as i32 - 1, (b[0].max(e[0]) + r).ceil() as i32 + 1);
                let (z0, z1) = ((b[2].min(e[1]) - r).floor() as i32 - 1, (b[2].max(e[1]) + r).ceil() as i32 + 1);
                for y in (b[1] - r).floor() as i32..=(b[1] + r).ceil() as i32 {
                    let dy = y as f32 + 0.5 - b[1];
                    for z in z0..=z1 {
                        for x in x0..=x1 {
                            let (u, v) = (x as f32 + 0.5 - b[0], z as f32 + 0.5 - b[2]);
                            let t = u * d[0] + v * d[1];
                            let across = u * -d[1] + v * d[0];
                            if (0.0..*length).contains(&t) && across * across + dy * dy <= r * r {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Log {
                base,
                radius,
                length,
                axis,
                mat,
                part,
                ..
            } => {
                let Some(m) = resolve(mat) else { continue };
                // Wood grain along the log.
                let m = crate::world::matlib::grained(m, if axis == "x" { crate::world::matlib::Axis::X } else { crate::world::matlib::Axis::Z });
                let b = addf(*base);
                let p = part_code(part.as_ref());
                let r = *radius;
                let along_x = axis == "x";
                for t in 0..length.ceil() as i32 {
                    for y in (b[1] - r).floor() as i32..=(b[1] + r).ceil() as i32 {
                        for w in -(r.ceil() as i32) - 1..=r.ceil() as i32 + 1 {
                            let (x, z, cw) = if along_x {
                                (b[0].floor() as i32 + t, b[2].floor() as i32 + w, b[2])
                            } else {
                                (b[0].floor() as i32 + w, b[2].floor() as i32 + t, b[0])
                            };
                            let across = if along_x { z as f32 + 0.5 - cw } else { x as f32 + 0.5 - cw };
                            if across.powi(2) + (y as f32 + 0.5 - b[1]).powi(2) <= r * r {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Disc {
                centre,
                radius,
                thick,
                axis,
                mat,
                part,
                yaw,
            } if *yaw != 0.0 => {
                let Some(m) = resolve(mat) else { continue };
                let n = turned_axis(axis, *yaw);
                let c = addf(*centre);
                let p = part_code(part.as_ref());
                let r = *radius;
                let ri = r.ceil() as i32 + 2;
                let half = (thick * 0.5).max(0.5);
                for y in c[1] as i32 - ri..=c[1] as i32 + ri {
                    let dy = y as f32 + 0.5 - c[1];
                    for z in c[2] as i32 - ri..=c[2] as i32 + ri {
                        for x in c[0] as i32 - ri..=c[0] as i32 + ri {
                            let (u, v) = (x as f32 + 0.5 - c[0], z as f32 + 0.5 - c[2]);
                            let w = u * n[0] + v * n[1];
                            let side = u * -n[1] + v * n[0];
                            if w.abs() <= half && side * side + dy * dy <= r * r {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Disc {
                centre,
                radius,
                thick,
                axis,
                mat,
                part,
                ..
            } => {
                let Some(m) = resolve(mat) else { continue };
                let c = addf(*centre);
                let p = part_code(part.as_ref());
                let r = *radius;
                let along_x = axis == "x";
                let ri = r.ceil() as i32 + 1;
                let ti = (thick * 0.5).ceil() as i32;
                for y in c[1] as i32 - ri..=c[1] as i32 + ri {
                    for u in c[if along_x { 2 } else { 0 }] as i32 - ri..=c[if along_x { 2 } else { 0 }] as i32 + ri {
                        for w in -ti..ti.max(1) {
                            let (x, z) = if along_x { (c[0] as i32 + w, u) } else { (u, c[2] as i32 + w) };
                            let du = u as f32 + 0.5 - c[if along_x { 2 } else { 0 }];
                            if du * du + (y as f32 + 0.5 - c[1]).powi(2) <= r * r {
                                put(s, x, y, z, m, p);
                            }
                        }
                    }
                }
            }
            Op::Set { at, mat, part } => {
                let Some(m) = resolve(mat) else { continue };
                let a = add(*at);
                put(s, a[0], a[1], a[2], m, part_code(part.as_ref()));
            }
            Op::Segment { points, radius, mat, part } => {
                let Some(m) = resolve(mat) else { continue };
                let p = part_code(part.as_ref());
                // One line of voxels joined at least at their corners, as
                // stamp.rs draws a tube limb: the thread covers exactly these.
                let hull = radius.max(0.62);
                for w in points.windows(2) {
                    let (a, b) = (addf(w[0]), addf(w[1]));
                    let lo = |k: usize| (a[k].min(b[k]) - hull).floor() as i32;
                    let hi = |k: usize| (a[k].max(b[k]) + hull).ceil() as i32;
                    let d = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let l2 = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).max(1e-6);
                    for y in lo(1)..=hi(1) {
                        for z in lo(2)..=hi(2) {
                            for x in lo(0)..=hi(0) {
                                let c = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                                let t = (((c[0] - a[0]) * d[0] + (c[1] - a[1]) * d[1] + (c[2] - a[2]) * d[2]) / l2).clamp(0.0, 1.0);
                                let q = [a[0] + d[0] * t - c[0], a[1] + d[1] * t - c[1], a[2] + d[2] * t - c[2]];
                                if q[0] * q[0] + q[1] * q[1] + q[2] * q[2] < hull * hull {
                                    // A solid segment fills only air (or a
                                    // carve mark): what it passes through
                                    // keeps its own voxels, and its tube is
                                    // drawn over them.
                                    let Some(i) = s.idx(x, y, z) else { continue };
                                    if m == 0 {
                                        put(s, x, y, z, m, p);
                                    } else if s.mat[i] == 0 || s.mat[i] == CARVE {
                                        put(s, x, y, z, m, p);
                                        if s.owned.is_empty() {
                                            s.owned = vec![false; s.mat.len()];
                                        }
                                        s.owned[i] = true;
                                    }
                                }
                            }
                        }
                    }
                    if m != 0 {
                        s.threads.push(ShapeThread { a, b, r: *radius, hull, mat: m });
                    }
                }
            }
            Op::RecolourBelow { y, from, to } => {
                let (Some(f), Some(t)) = (resolve(from), resolve(to)) else {
                    continue;
                };
                for i in 0..s.mat.len() {
                    let yy = i as i32 / (s.size[0] * s.size[2]);
                    if yy < *y + off[1] && s.mat[i] == f {
                        s.mat[i] = t;
                    }
                }
            }
            Op::Recolour { at, until, from, to } => {
                let (Some(f), Some(t)) = (resolve(from), resolve(to)) else {
                    continue;
                };
                let (a, b) = (add(*at), add(*until));
                for y in a[1]..b[1] {
                    for z in a[2]..b[2] {
                        for x in a[0]..b[0] {
                            if let Some(i) = s.idx(x, y, z) {
                                if s.mat[i] == f {
                                    s.mat[i] = t;
                                }
                            }
                        }
                    }
                }
            }
            Op::Repeat { count, step, ops } => {
                for k in 0..*count {
                    run(s, ops, [off[0] + step[0] * k, off[1] + step[1] * k, off[2] + step[2] * k], resolve);
                }
            }
        }
    }
}

/// Rotate a local voxel by quarter turns about y within a footprint of `size`.
/// Returns the rotated local coordinate in the rotated footprint.
pub fn rotate(x: i32, z: i32, size: [i32; 3], rot: u8) -> (i32, i32) {
    match rot & 3 {
        0 => (x, z),
        1 => (size[2] - 1 - z, x),
        2 => (size[0] - 1 - x, size[2] - 1 - z),
        _ => (z, size[0] - 1 - x),
    }
}

/// Inverse of `rotate`: from a rotated-footprint coordinate back to local.
pub fn unrotate(rx: i32, rz: i32, size: [i32; 3], rot: u8) -> (i32, i32) {
    match rot & 3 {
        0 => (rx, rz),
        1 => (rz, size[2] - 1 - rx),
        2 => (size[0] - 1 - rx, size[2] - 1 - rz),
        _ => (size[0] - 1 - rz, rx),
    }
}

/// Footprint (x, z) size after rotation.
pub fn rotated_size(size: [i32; 3], rot: u8) -> (i32, i32) {
    if rot & 1 == 1 {
        (size[2], size[0])
    } else {
        (size[0], size[2])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_round_trips() {
        let size = [5, 3, 8];
        for rot in 0..4u8 {
            let (w, d) = rotated_size(size, rot);
            for z in 0..size[2] {
                for x in 0..size[0] {
                    let (rx, rz) = rotate(x, z, size, rot);
                    assert!(rx >= 0 && rz >= 0 && rx < w && rz < d, "rot {rot}: {rx},{rz} outside {w}x{d}");
                    assert_eq!(unrotate(rx, rz, size, rot), (x, z));
                }
            }
        }
    }

    #[test]
    fn walls_and_gable_compile() {
        let ops: Vec<Op> = serde_json::from_str(
            r#"[
            {"op":"walls","from":[0,0,0],"to":[8,4,6],"mat":"w"},
            {"op":"box","from":[3,0,0],"to":[5,3,1],"mat":"air"},
            {"op":"gable","from":[0,4,0],"to":[8,4,6],"axis":"x","mat":"r","part":"roof"}
        ]"#,
        )
        .unwrap();
        let s = compile(&ops, [8, 10, 6], &|n| match n {
            "w" => Some(5),
            "r" => Some(7),
            "air" => Some(0),
            _ => None,
        });
        assert_eq!(s.get(0, 0, 0).0, 5);
        assert_eq!(s.get(4, 1, 0).0, 0, "door carved");
        assert_eq!(s.get(3, 1, 3).0, 0, "inside is hollow");
        assert_eq!(s.get(2, 4, 0), (7, PART_ROOF));
        assert!(s.top >= 7, "roof rises: top {}", s.top);
    }
    /// A segment fills only air and owns only what it drew: a brace of the
    /// pole's own steel leaves the pole's voxels to the pole (they keep
    /// their faces), and later ops take voxels back from it.
    #[test]
    fn segments_own_only_the_air_they_fill() {
        let ops: Vec<Op> = serde_json::from_str(
            r#"[
            {"op":"box","from":[4,0,1],"to":[6,12,3],"mat":"steel"},
            {"op":"segment","points":[[4,4,2],[0,10,2]],"radius":0.8,"mat":"steel"},
            {"op":"box","from":[0,10,0],"to":[2,12,4],"mat":"steel"}
        ]"#,
        )
        .unwrap();
        let s = compile(&ops, [8, 12, 4], &|n| (n == "steel").then_some(3));
        let owned = |x: i32, y: i32, z: i32| s.thread_vox.contains(&(s.idx(x, y, z).unwrap() as u32));
        assert!(!s.thread_vox.is_empty(), "the brace owns its voxels");
        for y in 0..12 {
            for z in 1..3 {
                for x in 4..6 {
                    assert!(!owned(x, y, z), "pole voxel {x},{y},{z} taken by the brace");
                }
            }
        }
        assert!(owned(2, 7, 2) || owned(1, 7, 2) || owned(2, 6, 2), "brace middle owned");
        assert!(!owned(0, 10, 2) && !owned(1, 11, 2), "the later box took the brace's end back");
        assert_eq!(s.get(4, 4, 2).0, 3, "pole stands");
    }
}
