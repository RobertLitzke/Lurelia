//! `skeleton`: the bones of a giant lying on the land, drawn as volumes
//! (`world/bones.rs`). The spine follows `path` from the head to the tail
//! and lies on the ground; the animal is on its side, so its upper ribs
//! arch up and over toward the `belly` side and its lower ribs lie flat in
//! the sand under them. Sizes go by the length of the path.

use crate::world::bones::{Bone, BoneSet, Bones};
use crate::world::chronicle::geom::{smoothed_ground, spline};
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Poi, Pos};
use crate::world::noise::{hash32, unit};
use glam::{Vec2, Vec3};
use serde::Deserialize;
use std::f32::consts::PI;

#[derive(Deserialize, Debug)]
pub struct Skeleton {
    /// Names the places it leaves: `<name>_skull`, `_blowhole`, `_jaw`,
    /// `_ribs`, `_flipper`, `_scapula`, `_hips`, `_tail`, `_spine_1`.. `_9`.
    name: String,
    /// The spine, head first; the body is as long as the path.
    path: Vec<Pos>,
    /// "toothed" (teeth in the lower jaw) or "baleen" (rows of plates on an
    /// upturned upper jaw).
    #[serde(default = "default_kind")]
    kind: String,
    /// The skull's share of the length.
    #[serde(default = "default_skull")]
    skull: f32,
    #[serde(default = "default_vertebrae")]
    vertebrae: usize,
    #[serde(default = "default_ribs")]
    ribs: usize,
    /// The tallest rib's height over the spine, metres (0.14 of the length).
    #[serde(default)]
    rib_height: Option<f32>,
    /// How far the longest rib reaches across, metres (0.15 of the length).
    #[serde(default)]
    rib_span: Option<f32>,
    /// "right" or "left" of the line from head to tail: the side the ribs
    /// arch over to.
    #[serde(default = "default_belly")]
    belly: String,
    /// How far every rib's tip leans to the left of the line from head to
    /// tail, as a share of its height (negative leans right): the body
    /// settled to one side.
    #[serde(default)]
    lean: f32,
    /// Share of the upper ribs broken off in the air.
    #[serde(default = "default_broken")]
    broken: f32,
    /// Share of each vertebra's depth under the sand.
    #[serde(default = "default_sink")]
    sink: f32,
    /// Metres the skull has settled into the ground.
    #[serde(default)]
    skull_sink: f32,
    /// Where the fallen flipper and the shoulder blade lie, if not where
    /// the body would put them.
    #[serde(default)]
    flipper_at: Option<Pos>,
    #[serde(default)]
    scapula_at: Option<Pos>,
    /// Sand drifted into the gaps between the vertebrae, level with their
    /// worn tops, so the spine can be walked (or ridden) end to end.
    #[serde(default)]
    sand: Option<String>,
    bone: String,
    marrow: String,
    #[serde(default)]
    pits: Option<String>,
    #[serde(default)]
    lichen: Option<String>,
    #[serde(default = "default_lichen_share")]
    lichen_share: f32,
    #[serde(default)]
    wet: Option<String>,
    #[serde(default = "default_wet_depth")]
    wet_depth: f32,
    #[serde(default)]
    deep: Option<String>,
    /// Teeth or baleen.
    #[serde(default)]
    teeth: Option<String>,
    #[serde(default)]
    poi: Option<String>,
}

fn default_kind() -> String {
    "toothed".into()
}
fn default_skull() -> f32 {
    0.2
}
fn default_vertebrae() -> usize {
    50
}
fn default_ribs() -> usize {
    14
}
fn default_belly() -> String {
    "right".into()
}
fn default_broken() -> f32 {
    0.35
}
fn default_sink() -> f32 {
    0.6
}
fn default_lichen_share() -> f32 {
    0.3
}
fn default_wet_depth() -> f32 {
    5.0
}

/// A seeded draw in 0..1.
struct Dice(u32);
impl Dice {
    fn next(&mut self) -> f32 {
        self.0 = hash32(self.0.wrapping_add(0x9e37_79b9));
        unit(self.0)
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.next()
    }
}

fn v3(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
}

impl Process for Skeleton {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let pts: Vec<Vec2> = self.path.iter().map(|p| cx.pos(p)).collect();
        if pts.len() < 2 {
            cx.warnings.push(format!("skeleton {}: a path needs two points", self.name));
            return format!("skeleton {}: no path", self.name);
        }
        let mat = |cx: &mut EventCtx, m: &Option<String>| m.as_ref().map_or(0, |n| cx.mat(n));
        let set = BoneSet {
            bone: cx.mat(&self.bone),
            marrow: cx.mat(&self.marrow),
            pits: mat(cx, &self.pits),
            lichen: mat(cx, &self.lichen),
            lichen_share: self.lichen_share,
            wet: mat(cx, &self.wet),
            wet_depth: self.wet_depth,
            deep: mat(cx, &self.deep),
            shell: 0.9,
        };
        let tooth_mat = mat(cx, &self.teeth);
        let sand = mat(cx, &self.sand);
        let mut dice = Dice(cx.seed ^ 0x5ce1);
        let (cause, sea) = (cx.cause, cx.f.sea);

        // The spine at 1 m steps, with arc length and the ground under it.
        let line = spline(&pts, false, 1.0);
        let mut arc = vec![0f32; line.len()];
        for i in 1..line.len() {
            arc[i] = arc[i - 1] + line[i].distance(line[i - 1]);
        }
        let len = *arc.last().unwrap();
        let ground = smoothed_ground(&cx.f, &line, 30.0, false);
        let at = |s: f32| -> (Vec2, Vec2, f32) {
            let i = arc.partition_point(|&a| a < s).clamp(1, line.len() - 1);
            let t = ((s - arc[i - 1]) / (arc[i] - arc[i - 1]).max(1e-4)).clamp(0.0, 1.0);
            let p = line[i - 1].lerp(line[i], t);
            let d = (line[i] - line[i - 1]).normalize_or_zero();
            (p, d, ground[i - 1] + (ground[i] - ground[i - 1]) * t)
        };
        let side = if self.belly == "left" { -1.0 } else { 1.0 };
        // Right of a heading (x east, z south): north's right is east.
        let belly_of = |d: Vec2| Vec2::new(-d.y, d.x) * side;

        let mut bones = std::mem::take(cx.state.get::<Bones>());
        let sid = bones.add_set(set);
        // Baleen plates are about a voxel thin: they keep their own material
        // in and out of the water, since the barnacled bone is rounded and
        // rounding a plate that thin pinches it into a twisted ribbon.
        let plates = self.kind == "baleen" && tooth_mat != 0;
        let teeth_set = bones.add_set(BoneSet {
            bone: if tooth_mat != 0 { tooth_mat } else { set.bone },
            marrow: if tooth_mat != 0 { tooth_mat } else { set.marrow },
            wet: if plates { 0 } else { set.wet },
            wet_depth: set.wet_depth,
            deep: if plates { 0 } else { set.deep },
            shell: 0.4,
            ..Default::default()
        });
        let first = bones.list.len();
        let mut push = |b: Bone| bones.list.push(b);
        let mut occupy: Vec<(Vec2, f32)> = Vec::new();
        let mut places: Vec<(String, Vec2)> = Vec::new();

        let skull_len = self.skull * len;
        let n = self.vertebrae.max(4);
        let spacing = (len - skull_len) / n as f32;
        let rv0 = 0.0065 * len;
        let neck = 7.min(n / 6);
        let rib_h = self.rib_height.unwrap_or(0.12 * len);
        let rib_span = self.rib_span.unwrap_or(0.04 * len);
        let rib_r = (0.0062 * len).max(1.4);

        // Vertebrae: a centrum worn flat on top and half sunk, the
        // transverse processes lying out into the sand either side, and
        // here and there what is left of a neural spine.
        let mut tops = Vec::with_capacity(n);
        for k in 0..n {
            let u = k as f32 / n as f32;
            let prof = if k < neck {
                0.8
            } else if u < 0.55 {
                1.0
            } else {
                1.0 - 0.6 * ((u - 0.55) / 0.45)
            };
            let r = rv0 * prof;
            let s = skull_len + (k as f32 + 0.5) * spacing;
            let (p, d, g) = at(s);
            let sink = if u > 0.55 { self.sink * (1.0 - 0.45 * (u - 0.55) / 0.45) } else { self.sink };
            let c = v3(p, g + r * (1.0 - 2.0 * sink));
            let half = 0.39 * spacing;
            let t3 = v3(d, 0.0);
            let top = c.y + 0.62 * r;
            tops.push((s, top, r));
            push(Bone { top, ..Bone::capsule(c - t3 * half * 0.8, c + t3 * half * 0.8, r, r * 0.97, sid) });
            occupy.push((p, r + 1.0));
            if u > 0.85 {
                continue;
            }
            let lat = v3(Vec2::new(-d.y, d.x), 0.0);
            for sgn in [-1.0f32, 1.0] {
                if dice.next() < 0.15 {
                    continue;
                }
                let l = r * dice.range(1.3, 2.3);
                let a = c + lat * sgn * 0.6 * r;
                let tip = a + lat * sgn * l - Vec3::Y * 0.2 * l + t3 * dice.range(-0.1, 0.2) * l;
                push(Bone { flat: Vec3::Y, squash: 0.45, cut_b: dice.next() < 0.4, ..Bone::capsule(a, tip, 0.32 * r, 0.14 * r, sid) });
            }
            // Most neural spines are worn down to the centrum; one in three
            // still stands, leaning back toward the tail, often broken.
            if k % 3 == 1 && dice.next() < 0.8 {
                let h = r * dice.range(1.2, 3.0);
                let a = c + Vec3::Y * 0.4 * r;
                let tip = a + Vec3::Y * h + t3 * 0.45 * h + lat * dice.range(-0.15, 0.15) * h;
                push(Bone { flat: t3, squash: 0.4, cut_b: dice.next() < 0.6, ..Bone::capsule(a, tip, 0.34 * r, 0.12 * r, sid) });
            }
        }
        // Drifted sand between the vertebrae, level with their tops, with a
        // ramp off each side: walkable end to end, and a way up from the beach.
        if sand != 0 {
            let f = &mut cx.f;
            for (i, &p) in line.iter().enumerate() {
                let s = arc[i];
                if s < skull_len {
                    continue;
                }
                let k = (((s - skull_len) / spacing) - 0.5).floor().clamp(0.0, n as f32 - 2.0) as usize;
                let (s0, t0, r0) = tops[k];
                let (s1, t1, r1) = tops[(k + 1).min(n - 1)];
                let w = ((s - s0) / (s1 - s0).max(1e-3)).clamp(0.0, 1.0);
                let top = t0 + (t1 - t0) * w - 0.05;
                let r = r0 + (r1 - r0) * w;
                // 1 in the middle of a gap, 0 over the middle of a vertebra.
                let gap = smoothstep_gap(w);
                if top < sea + 0.4 || gap <= 0.0 {
                    continue;
                }
                let reach = r * (0.85 + 1.4 * gap);
                let n_cells = reach.ceil() as i32 + 1;
                for dz in -n_cells..=n_cells {
                    for dx in -n_cells..=n_cells {
                        let q = p + Vec2::new(dx as f32, dz as f32);
                        if q.x < 0.0 || q.y < 0.0 || q.x >= f.size || q.y >= f.size {
                            continue;
                        }
                        let dist = q.distance(p);
                        if dist > reach {
                            continue;
                        }
                        let flat = r * 0.8;
                        let want = if dist <= flat { top } else { top - (dist - flat) * 0.55 };
                        let j = f.idx(q.x, q.y);
                        if want > f.h[j] && f.h[j] > sea - 1.0 && f.water[j] != 1 {
                            f.h[j] = want;
                            f.surface[j] = sand;
                            f.cliff[j] = 0.0;
                            f.cause[j] = cause;
                        }
                    }
                }
            }
        }

        // Ribs: the animal lies on its belly and has sunk, so its ribs stand
        // up from both sides of the spine like tusks, bowing out and then in,
        // never meeting. No two alike: each its own height, reach, lean and
        // sweep, a wobble along its length, and many broken off.
        let nr = self.ribs.min(n.saturating_sub(neck + 1));
        let mut rib_mid = Vec2::ZERO;
        let mut tallest: f32 = 0.0;
        for i in 0..nr {
            let k = neck + i;
            let u = if nr > 1 { i as f32 / (nr - 1) as f32 } else { 0.5 };
            // Tallest a third of the way back, falling off toward the hips.
            let shape = (PI * (0.15 + 0.85 * u.powf(0.8))).sin().max(0.25);
            let (s, _, r) = tops[k];
            let (p, d, g) = at(s);
            let lat = v3(Vec2::new(-d.y, d.x), 0.0);
            let t3 = v3(d, 0.0);
            if i == nr / 2 {
                rib_mid = p;
            }
            // One shape per pair, so the two sides mirror each other; they
            // differ only a little in length, and in where they broke.
            let ph = rib_h * (0.5 + 0.5 * shape) * dice.range(0.85, 1.05);
            let pout = rib_span * (0.6 + 0.4 * shape) * dice.range(0.9, 1.1);
            let plean = dice.range(0.15, 0.3);
            let psweep = dice.range(0.0, 0.12);
            for sgn in [-1.0f32, 1.0] {
                if dice.next() < 0.06 {
                    continue;
                }
                let h = ph * dice.range(0.95, 1.05);
                let out = pout * dice.range(0.96, 1.04);
                let lean_in = plean * out;
                let sweep = psweep * h;
                let side = lat * sgn;
                let base = v3(p, g + r * (1.0 - 2.0 * self.sink)) + side * 1.15 * r;
                let c1 = base + side * out * 0.95 + Vec3::Y * h * 0.18;
                let tilt = -lat * self.lean * h;
                let c2 = base + side * out * 1.05 + Vec3::Y * h * 0.72 + t3 * sweep * 0.4 + tilt * 0.5;
                let c3 = base + side * (out * 0.55 - lean_in) + Vec3::Y * h + t3 * sweep + tilt;
                let broken = dice.next() < self.broken;
                let end = if broken { dice.range(0.4, 0.85) } else { 1.0 };
                let wob = (dice.range(0.0, 6.28), dice.range(0.002, 0.006) * h);
                let segs = 24;
                let rb = rib_r * dice.range(0.85, 1.15) * (0.7 + 0.3 * shape);
                let pt = |t: f32| {
                    let m = 1.0 - t;
                    let q = base * (m * m * m) + c1 * (3.0 * m * m * t) + c2 * (3.0 * m * t * t) + c3 * (t * t * t);
                    q + t3 * (wob.1 * (t * 9.0 + wob.0).sin()) + side * (wob.1 * 0.6 * (t * 6.0 + wob.0 * 1.7).sin())
                };
                // The head of the rib: a knob where it met the spine.
                push(Bone::capsule(base - side * 0.3 * rb, base + side * 0.5 * rb + Vec3::Y * 0.3 * rb, rb * 1.25, rb * 1.1, sid));
                let mut prev = pt(0.0);
                for j in 1..=segs {
                    let (w0, w1) = ((j - 1) as f32 / segs as f32 * end, j as f32 / segs as f32 * end);
                    let cur = pt(w1);
                    let dir = (cur - prev).normalize_or_zero();
                    let flat = dir.cross(t3).normalize_or_zero();
                    let rr = |w: f32| (rb * (1.0 - 0.86 * w.powf(1.3))).max(0.5);
                    push(Bone { flat, squash: 0.62, cut_b: broken && j == segs, ..Bone::capsule(prev, cur, rr(w0), rr(w1), sid) });
                    prev = cur;
                }
                tallest = tallest.max(prev.y - sea);
                if broken && dice.next() < 0.7 {
                    // The piece that fell, lying beside the rib.
                    let lie = Vec2::new(prev.x, prev.z) + Vec2::new(side.x, side.z) * out * dice.range(0.2, 0.6) + d * dice.range(-20.0, 20.0);
                    let gl = cx.f.height(lie.x, lie.y);
                    let ang = dice.range(0.0, 6.28);
                    let along = Vec3::new(ang.cos(), 0.0, ang.sin());
                    let half = (1.0 - end) * h * dice.range(0.25, 0.45);
                    let cc = v3(lie, gl + rb * 0.15);
                    push(Bone { flat: Vec3::Y, squash: 0.62, cut_a: true, cut_b: true, ..Bone::capsule(cc - along * half, cc + along * half, rb * 0.42, rb * 0.34, sid) });
                }
            }
        }

        // The fallen flipper: humerus, forearm and four fingers fanned out
        // flat on the ground ahead of the ribs, and the shoulder blade.
        if nr > 0 {
            let (s, _, r) = tops[neck];
            let (p, d, _) = at(s);
            let bl = belly_of(d);
            let out = (bl * 0.7 - d * 0.7).normalize();
            let root = self.flipper_at.as_ref().map(|q| cx.pos(q)).unwrap_or(p + bl * (r * 2.0 + 0.04 * len));
            let lay = |cx: &EventCtx, q: Vec2, lift: f32| v3(q, cx.f.height(q.x, q.y) + lift);
            let hl = 0.03 * len;
            let hr = 0.0055 * len;
            let elbow = root + out * hl;
            push(Bone { cut_b: false, ..Bone::capsule(lay(cx, root, hr * 0.2), lay(cx, elbow, hr * 0.15), hr, hr * 0.75, sid) });
            let wrist = elbow + out * hl * 0.8;
            for o in [-0.35f32, 0.35] {
                let side2 = Vec2::new(-out.y, out.x) * o * hr;
                push(Bone::capsule(lay(cx, elbow + side2, hr * 0.1), lay(cx, wrist + side2 * 1.3, hr * 0.1), hr * 0.45, hr * 0.4, sid));
            }
            for f in 0..4 {
                let ang = (f as f32 - 1.5) * 0.34;
                let dir = Vec2::new(out.x * ang.cos() - out.y * ang.sin(), out.x * ang.sin() + out.y * ang.cos());
                let mut a = wrist + Vec2::new(-out.y, out.x) * (f as f32 - 1.5) * hr * 1.3;
                let mut rr = hr * 0.3;
                for _ in 0..4 {
                    let l = hl * 0.32 * dice.range(0.8, 1.1);
                    let c = a + dir * l;
                    push(Bone { cut_a: true, cut_b: true, ..Bone::capsule(lay(cx, a, rr * 0.2), lay(cx, c, rr * 0.2), rr, rr * 0.85, sid) });
                    a = c + dir * hr * 0.15;
                    rr *= 0.82;
                }
            }
            occupy.push((root + out * hl, hl * 1.2));
            places.push((format!("{}_flipper", self.name), elbow));
            // The shoulder blade: a dish lying face up, rim proud of the
            // sand; at the waterline the sea stands in it.
            let sc = self.scapula_at.as_ref().map(|q| cx.pos(q)).unwrap_or(root - out * hl * 0.9 + bl * hl * 0.3);
            let sr = 0.032 * len;
            let gy = cx.f.height(sc.x, sc.y);
            let axis = v3(out, 0.0) * sr * 0.25;
            let c = v3(sc, gy + 0.2);
            push(Bone { flat: Vec3::Y, squash: 0.07, ..Bone::capsule(c - axis, c + axis, sr, sr * 0.9, sid) });
            let cc = c + Vec3::Y * (sr * 0.07 * 0.6);
            push(Bone { flat: Vec3::Y, squash: 0.05, carve: true, ..Bone::capsule(cc - axis * 0.85, cc + axis * 0.85, sr * 0.82, sr * 0.74, sid) });
            occupy.push((sc, sr));
            places.push((format!("{}_scapula", self.name), sc));
        }

        // The skull, settled upright at the head of the spine: a tall
        // braincase narrowing forward into a long snout, lifted so the mouth
        // gapes, over a lower jaw lying on the ground.
        let (o2, d_occ, _) = at(skull_len);
        let fwd2 = -d_occ;
        let f3 = v3(fwd2, 0.0);
        let s3 = v3(Vec2::new(-fwd2.y, fwd2.x), 0.0);
        let rc = 0.15 * skull_len;
        let cxz = o2 + fwd2 * 0.16 * skull_len;
        let gc = cx.f.height(cxz.x, cxz.y);
        let lift = |q: Vec2, h: f32| v3(q, cx.f.height(q.x, q.y) + h);
        let bxz = o2 + fwd2 * 0.03 * skull_len;
        let fxz = o2 + fwd2 * 0.32 * skull_len;
        let back = v3(bxz, gc + 0.55 * rc - self.skull_sink);
        let front = v3(fxz, gc + 0.45 * rc - self.skull_sink);
        let rf = 0.62 * rc;
        let shell = (0.12 * rc).max(1.0);
        // Lower than it is wide, with a crest along the top, knobs at the
        // back and a bar of cheekbone under each eye.
        push(Bone { flat: Vec3::Y, squash: 0.55, ..Bone::capsule(back, front, rc, rf, sid) });
        push(Bone { flat: s3, squash: 0.45, ..Bone::capsule(back + Vec3::Y * 0.36 * rc - f3 * 0.3 * rc, front + Vec3::Y * 0.36 * rf, 0.2 * rc, 0.1 * rc, sid) });
        for sgn in [-1.0f32, 1.0] {
            let k = back - f3 * 0.75 * rc + s3 * sgn * 0.35 * rc + Vec3::Y * 0.15 * rc;
            push(Bone::capsule(k, k, 0.24 * rc, 0.24 * rc, sid));
            let c0 = back + s3 * sgn * 0.75 * rc - Vec3::Y * 0.05 * rc;
            let c1 = front + f3 * 0.35 * rc + s3 * sgn * 0.55 * rf - Vec3::Y * 0.05 * rc;
            push(Bone { flat: Vec3::Y, squash: 0.6, ..Bone::capsule(c0, c1, 0.13 * rc, 0.09 * rc, sid) });
        }
        let carve = |a: Vec3, b: Vec3, ra: f32, rb: f32| Bone { carve: true, ..Bone::capsule(a, b, ra, rb, sid) };
        push(Bone { flat: Vec3::Y, squash: 0.55, ..carve(back, front, rc - shell, rf - shell) });
        // A low doorway in the back of the braincase.
        let door = back - f3 * 0.95 * rc - Vec3::Y * 0.35 * rc;
        push(carve(door, door + f3 * 0.4 * rc, 0.3 * rc, 0.3 * rc));
        // Eyes sunk under heavy brows. Each socket's middle is a named place
        // (`<name>_eye_left` and `_eye_right`, as the skull faces), its height
        // over the sea and its radius in the summary, for something to be
        // set in it with `place` `height`.
        let mut eyes = Vec::new();
        for sgn in [-1.0f32, 1.0] {
            let e = back.lerp(front, 0.62) + s3 * sgn * 0.62 * rc + Vec3::Y * 0.18 * rc;
            eyes.push((if sgn > 0.0 { "right" } else { "left" }, e + s3 * sgn * 0.05 * rc, 0.18 * rc));
            push(carve(e - s3 * sgn * 0.1 * rc, e + s3 * sgn * 0.2 * rc, 0.17 * rc, 0.19 * rc));
            // A heavy brow ridge over the socket, half sunk in the braincase
            // so it grows out of it (a flat shelf here read as a lid floating
            // clear of the skull).
            let brow = e + Vec3::Y * 0.1 * rc - s3 * sgn * 0.04 * rc;
            push(Bone::capsule(brow - f3 * 0.3 * rc, brow + f3 * 0.32 * rc, 0.13 * rc, 0.1 * rc, sid));
        }
        let bh = back.lerp(front, 0.3);
        push(carve(bh + Vec3::Y * 0.2 * rc, bh + Vec3::Y * 1.0 * rc, 0.1 * rc, 0.1 * rc));
        let baleen = self.kind == "baleen";
        let snout_len = (1.0 - 0.32) * skull_len;
        // The upper jaw, lifted about twenty degrees: the mouth stands open.
        let (snout0, tip) = if baleen {
            // Upturned and lying flat, palate up.
            let tip_xz = fxz + fwd2 * snout_len;
            (lift(fxz, 0.2 * rc), lift(tip_xz, 0.2 * rc))
        } else {
            let a = front + Vec3::Y * 0.05 * rc;
            (a, a + f3 * snout_len * 0.94 + Vec3::Y * snout_len * 0.34)
        };
        let bend = snout0.lerp(tip, 0.55) - Vec3::Y * snout_len * if baleen { 0.0 } else { 0.06 };
        let snout_at = |w: f32| if w < 0.55 { snout0.lerp(bend, w / 0.55) } else { bend.lerp(tip, (w - 0.55) / 0.45) };
        let srad = |w: f32| rf * 0.85 * (1.0 - w) + 0.04 * rc * w;
        let sq = if baleen { 0.4 } else { 0.62 };
        push(Bone { flat: Vec3::Y, squash: sq, ..Bone::capsule(snout0, bend, srad(0.0), srad(0.55), sid) });
        push(Bone { flat: Vec3::Y, squash: sq, ..Bone::capsule(bend, tip, srad(0.55), srad(1.0), sid) });
        if !baleen {
            let up = |w: f32| snout_at(w) + Vec3::Y * srad(w) * sq * 0.85;
            push(Bone { flat: s3, squash: 0.4, ..Bone::capsule(up(0.0), up(0.55), 0.18 * rc, 0.1 * rc, sid) });
            push(Bone { flat: s3, squash: 0.4, ..Bone::capsule(up(0.55), up(0.95), 0.1 * rc, 0.03 * rc, sid) });
        }
        // A tooth: a curved cone in two pieces, hooked back toward the throat.
        let tooth = |base: Vec3, dir: Vec3, out: Vec3, hgt: f32, broken: bool| -> [Bone; 2] {
            let mid = base + dir * 0.55 * hgt + out * 0.08 * hgt;
            let tp = base + dir * hgt - f3 * 0.22 * hgt + out * 0.1 * hgt;
            [Bone::capsule(base, mid, 0.16 * hgt, 0.1 * hgt, teeth_set), Bone { cut_b: broken, ..Bone::capsule(mid, tp, 0.12 * hgt, 0.02 * hgt, teeth_set) }]
        };
        if !baleen && tooth_mat != 0 {
            // Upper teeth hanging from both edges, biggest toward the front.
            for sgn in [-1.0f32, 1.0] {
                let count = 15;
                for t in 0..count {
                    if dice.next() < 0.18 {
                        continue;
                    }
                    let w = 0.06 + 0.88 * (t as f32 + 0.5 * (sgn + 1.0) * 0.5) / count as f32;
                    let q = snout_at(w);
                    let rad = srad(w);
                    let hgt = 0.07 * skull_len * (0.65 + 0.55 * w) * dice.range(0.8, 1.1);
                    let base = q + s3 * sgn * rad * 0.7 - Vec3::Y * rad * 0.4;
                    let broke = dice.next() < 0.15;
                    for b in tooth(base, -Vec3::Y, s3 * sgn, hgt, broke) {
                        push(b);
                    }
                }
            }
        }
        // The lower jaw: two solid bars from under the braincase, meeting at
        // the chin.
        let rm = 0.16 * rc;
        let chin_xz = fxz + fwd2 * snout_len * 0.92;
        let chin = lift(chin_xz, 0.5 * rm - self.skull_sink * 0.2);
        let mut jaw_mid = cxz;
        for sgn in [-1.0f32, 1.0] {
            let s2 = Vec2::new(-fwd2.y, fwd2.x) * sgn;
            let a = lift(bxz + s2 * 0.6 * rc, 0.6 * rm - self.skull_sink * 0.3);
            let mxz = fxz + fwd2 * snout_len * 0.4 + s2 * 0.4 * rc;
            let m = lift(mxz, 0.55 * rm - self.skull_sink * 0.3);
            let e = chin + s3 * sgn * 0.6 * rm;
            push(Bone::capsule(a, m, rm * 1.15, rm, sid));
            push(Bone::capsule(m, e, rm, rm * 0.7, sid));
            jaw_mid = mxz;
            if tooth_mat == 0 {
                continue;
            }
            if baleen {
                // Plates in a row along each edge of the upturned snout.
                let count = (0.8 * snout_len / 0.9).max(4.0) as usize;
                for t in 0..count {
                    let w = 0.1 + 0.85 * t as f32 / count as f32;
                    let q = snout_at(w) + s3 * sgn * rf * 0.7 * (1.0 - 0.7 * w);
                    let hgt = 0.07 * skull_len * (1.0 - 0.5 * w) * dice.range(0.8, 1.05);
                    let base = q + Vec3::Y * 0.2 * rf;
                    push(Bone { flat: f3, squash: 0.12, ..Bone::capsule(base, base + Vec3::Y * hgt - s3 * sgn * 0.15 * hgt, 0.35 * hgt, 0.04 * hgt, teeth_set) });
                }
            } else {
                // Lower teeth up the front of each bar, set between the upper ones.
                let count = 13;
                for t in 0..count {
                    if dice.next() < 0.22 {
                        continue;
                    }
                    let w = 0.3 + 0.66 * (t as f32 + 0.5) / count as f32;
                    let q = if w < 0.5 { a.lerp(m, w / 0.5) } else { m.lerp(e, (w - 0.5) / 0.5) };
                    let hgt = 0.065 * skull_len * (0.6 + 0.6 * w) * dice.range(0.8, 1.1);
                    let broke = dice.next() < 0.2;
                    for b in tooth(q + Vec3::Y * rm * 0.6, Vec3::Y, s3 * sgn, hgt, broke) {
                        push(b);
                    }
                }
            }
        }
        push(Bone::capsule(chin - s3 * 0.6 * rm, chin + s3 * 0.6 * rm, rm * 0.8, rm * 0.8, sid));
        occupy.push((cxz, rc * 1.1));
        occupy.push((o2 + fwd2 * 0.6 * skull_len, rc * 0.6));
        places.push((format!("{}_skull", self.name), cxz));
        let mut eye_note = String::new();
        for (side, c, r) in &eyes {
            places.push((format!("{}_eye_{side}", self.name), Vec2::new(c.x, c.z)));
            eye_note += &format!("; {side} eye socket {:.0},{:.0} at {:.1} m over the sea, {:.1} m across", c.x, c.z, c.y - cx.f.sea, r * 2.0);
        }
        places.push((format!("{}_blowhole", self.name), Vec2::new(bh.x, bh.z)));
        places.push((format!("{}_jaw", self.name), jaw_mid));
        if nr > 0 {
            places.push((format!("{}_ribs", self.name), rib_mid));
        }
        let hips = at(skull_len + spacing * (n as f32 * 0.62)).0;
        places.push((format!("{}_hips", self.name), hips));
        places.push((format!("{}_tail", self.name), at(len - spacing).0));
        for k in 1..=9 {
            places.push((format!("{}_spine_{k}", self.name), at(skull_len + (len - skull_len) * k as f32 / 10.0).0));
        }

        let count = bones.list.len() - first;
        *cx.state.get::<Bones>() = bones;
        let f = &mut cx.f;
        for (c, r) in occupy {
            let ri = r.ceil() as i32;
            for dz in -ri..=ri {
                for dx in -ri..=ri {
                    let q = c + Vec2::new(dx as f32, dz as f32);
                    if q.x >= 0.0 && q.y >= 0.0 && q.x < f.size && q.y < f.size && q.distance(c) <= r {
                        let j = f.idx(q.x, q.y);
                        f.occupied[j] = true;
                    }
                }
            }
        }
        let mut names = Vec::new();
        for (nm, p) in &places {
            cx.seat(nm, *p, None);
            names.push(format!("{nm} {:.0},{:.0}", p.x, p.y));
        }
        if let Some(poi) = &self.poi {
            cx.pois.push(Poi { name: poi.clone(), pos: cxz, cause });
        }
        format!(
            "skeleton {}: {:.0} m long, {count} bones, {n} vertebrae, {nr} ribs, tallest {:.0} m over the sea{eye_note}; places {}",
            self.name,
            len,
            tallest,
            names.join(", ")
        )
    }
}

/// How much of a gap between two vertebrae w (0..1 from one centre to the
/// next) is in: 0 over a vertebra's middle two thirds, 1 in the gap.
fn smoothstep_gap(w: f32) -> f32 {
    let e = (w - 0.5).abs() * 2.0; // 1 at the centres, 0 midway
    crate::world::noise::smoothstep(0.75, 0.35, e)
}
