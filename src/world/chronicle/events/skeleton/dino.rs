//! The dinosaur kinds of `skeleton`: "sauropod" (a long-neck) and
//! "theropod" (a big hunter on two legs). The spine follows the path from
//! the snout to the tip of the tail, so bending the path curls the neck back
//! the way most long-necks died. In the "side" pose the animal lies on its
//! `belly`-ward side: the upper ribs arch up and over and come down on the
//! belly side, the lower ribs lie flat on the ground under them, the neural
//! spines lie out the other way and all four legs lie on the ground toward
//! the belly. In the "belly" pose it lies face down: the spines stand up,
//! the ribs hoop out either side and the legs are splayed. `sink` buries
//! both. Sizes go by the length of the path.

use super::{bent_bone, cubic, opt_mat, settle, v3, Dice, Skeleton, Spine};
use crate::world::chronicle::geom::cumulative;
use crate::world::bones::{Bone, BoneSet, Bones};
use crate::world::chronicle::EventCtx;
use crate::world::fields::Poi;
use glam::{Vec2, Vec3};
use std::f32::consts::PI;

/// Thinner than this is under a voxel and drops out.
const MIN_R: f32 = 0.12;

/// Proportions, as shares of the length along the bones.
struct Plan {
    skull: f32,
    neck: f32,
    trunk: f32,
    n_neck: usize,
    n_trunk: usize,
    n_tail: usize,
    /// Centrum radius in the trunk.
    centrum: f32,
    /// Neck centrum radius at the head and at the shoulders, in trunk centra.
    neck_r: (f32, f32),
    /// Neural spine height in centra: neck, trunk, tail root.
    spine: (f32, f32, f32),
    /// How far the ribs reach from the spine to the belly, how far out to
    /// the side the cage bulges, and their thickness.
    rib_depth: f32,
    rib_width: f32,
    rib_r: f32,
    /// Bones of the fore and hind limb from the girdle out: (length, radius).
    arm: [(f32, f32); 3],
    leg: [(f32, f32); 3],
    /// Fingers and toes: how many, and the length of each bone.
    hand: (usize, f32),
    foot: (usize, f32),
    claws: bool,
    /// Length and half width of the hip blade, and of the shoulder blade.
    ilium: (f32, f32),
    scapula: (f32, f32),
    /// Long thin ribs running back along the neck.
    cervical_ribs: bool,
}

const SAUROPOD: Plan = Plan {
    skull: 0.035,
    neck: 0.36,
    trunk: 0.19,
    n_neck: 15,
    n_trunk: 13,
    n_tail: 42,
    centrum: 0.011,
    neck_r: (0.45, 0.9),
    spine: (0.9, 2.4, 1.7),
    rib_depth: 0.085,
    rib_width: 0.075,
    rib_r: 0.0045,
    arm: [(0.06, 0.011), (0.045, 0.008), (0.016, 0.0065)],
    leg: [(0.07, 0.013), (0.046, 0.009), (0.016, 0.008)],
    hand: (4, 0.008),
    foot: (4, 0.011),
    claws: false,
    ilium: (0.09, 0.034),
    scapula: (0.07, 0.018),
    cervical_ribs: true,
};

const THEROPOD: Plan = Plan {
    skull: 0.12,
    neck: 0.1,
    trunk: 0.24,
    n_neck: 9,
    n_trunk: 14,
    n_tail: 36,
    centrum: 0.016,
    neck_r: (0.75, 0.9),
    spine: (1.0, 2.0, 1.6),
    rib_depth: 0.1,
    rib_width: 0.075,
    rib_r: 0.006,
    arm: [(0.03, 0.006), (0.02, 0.005), (0.012, 0.004)],
    leg: [(0.1, 0.014), (0.095, 0.011), (0.055, 0.008)],
    hand: (2, 0.012),
    foot: (3, 0.028),
    claws: true,
    ilium: (0.1, 0.03),
    scapula: (0.06, 0.01),
    cervical_ribs: false,
};

/// Turn `out` toward `back` (a unit vector at right angles to it) by `a` radians.
fn turn(out: Vec2, back: Vec2, a: f32) -> Vec2 {
    (out * a.cos() + back * a.sin()).normalize_or_zero()
}

/// A bone bent along the cubic curve with control points `c`, tapering
/// from radius `r0` at the root to `r1` (`bent_bone`).
#[allow(clippy::too_many_arguments)]
fn curved(list: &mut Vec<Bone>, c: [Vec3; 4], end: f32, r0: f32, r1: f32, along: Vec3, squash: f32, sid: u16) -> Vec3 {
    bent_bone(list, |t| cubic(c, t), 14, end, |w| (r0 + (r1 - r0) * w.powf(1.3)).max(MIN_R), along, squash, sid)
}

pub(super) fn lay(sk: &Skeleton, cx: &mut EventCtx, pts: &[Vec2]) -> String {
    let sauropod = sk.kind == "sauropod";
    let plan = if sauropod { &SAUROPOD } else { &THEROPOD };
    let on_side = sk.pose.as_deref() != Some("belly");
    // A dinosaur's bones are thin-walled next to a whale's.
    let set = sk.bone_set(cx, 0.3);
    let tooth_mat = opt_mat(cx, &sk.teeth);
    let mut dice = Dice(cx.seed ^ 0xd1_05a0);
    let cause = cx.cause;

    // The ground is smoothed over a window that goes by the path as given.
    let (_, path_len) = cumulative(pts, false);
    let spine = Spine::new(&cx.f, pts, (path_len * 0.12).clamp(4.0, 30.0));
    let len = spine.len();
    let at = |s: f32| spine.at(s);
    let bside = if sk.belly == "left" { -1.0 } else { 1.0 };

    let mut bones = std::mem::take(cx.state.get::<Bones>());
    let sid = bones.add_set(set);
    let teeth_set = bones.add_set(BoneSet {
        bone: if tooth_mat != 0 { tooth_mat } else { set.bone },
        marrow: if tooth_mat != 0 { tooth_mat } else { set.marrow },
        shell: 0.2,
        ..set
    });
    // The flat bones (shoulder and hip blades) are squashed thinner than
    // the cortex, so they are bone right through.
    let plates = bones.add_set(BoneSet { shell: 2.0, ..set });
    let first = bones.list.len();
    let list = &mut bones.list;
    let mut occupy: Vec<(Vec2, f32)> = Vec::new();
    let mut places: Vec<(String, Vec2)> = Vec::new();
    let f = &cx.f;
    let ground = |q: Vec2| f.height(q.x, q.y);

    let s_skull = sk.skull.unwrap_or(plan.skull) * len;
    let s_neck = s_skull + plan.neck * len;
    let s_trunk = s_neck + plan.trunk * len;
    let rt = (plan.centrum * len).max(MIN_R);
    let sink = sk.sink;
    let tall = |r: f32| r * (1.0 - 2.0 * sink);
    let trunk_gap = (s_trunk - s_neck) / plan.n_trunk as f32;

    // The frame at s: the point, the heading toward the tail, the ground,
    // the animal's right, the belly side, and its back (dorsal) direction.
    struct Frame {
        p: Vec2,
        d: Vec2,
        g: f32,
        lat: Vec2,
        bl: Vec2,
        dorsal: Vec3,
    }
    let frame = |s: f32| {
        let (p, d, g) = at(s);
        let lat = Vec2::new(-d.y, d.x);
        let bl = lat * bside;
        let dorsal = if on_side { -v3(bl, 0.0) } else { Vec3::Y };
        Frame { p, d, g, lat, bl, dorsal }
    };

    // Vertebrae: centra in a row down the path, each with what is left of
    // its neural spine, its side processes, under the tail its chevron and
    // along a long-neck's neck its thin neck ribs.
    let mut cent: Vec<(f32, Vec3, f32)> = Vec::new();
    let mut tallest: f32 = 0.0;
    for (sec, s0, s1, n) in [(0, s_skull, s_neck, plan.n_neck), (1, s_neck, s_trunk, plan.n_trunk), (2, s_trunk, len, plan.n_tail)] {
        let gap = (s1 - s0) / n as f32;
        for k in 0..n {
            let u = (k as f32 + 0.5) / n as f32;
            let s = s0 + (k as f32 + 0.5) * gap;
            let r = match sec {
                0 => rt * (plan.neck_r.0 + (plan.neck_r.1 - plan.neck_r.0) * u),
                1 => rt,
                _ => rt * (0.08 + 0.92 * (1.0 - u).powf(1.3)),
            }
            .max(MIN_R * 0.9);
            let fr = frame(s);
            let t3 = v3(fr.d, 0.0);
            let c = v3(fr.p, fr.g + tall(r));
            cent.push((s, c, r));
            list.push(Bone::capsule(c - t3 * gap * 0.4, c + t3 * gap * 0.4, r, r * 0.97, sid));
            occupy.push((fr.p, r + 0.5));
            tallest = tallest.max(c.y + r - fr.g);
            let sp = match sec {
                0 => plan.spine.0 * r,
                1 => plan.spine.1 * r,
                _ => plan.spine.2 * r * (1.0 - u / 0.75).max(0.0),
            };
            if sp > 0.4 * r && dice.next() > 0.1 {
                let broken = dice.next() < sk.broken * 0.6;
                let h = sp * if broken { dice.range(0.4, 0.8) } else { 1.0 };
                let a = c + fr.dorsal * 0.7 * r;
                let tip = a + fr.dorsal * h + t3 * 0.3 * h;
                list.push(Bone { flat: t3, squash: 0.45, cut_b: broken, ..Bone::capsule(a, tip, (0.32 * r).max(MIN_R), (0.18 * r).max(MIN_R), sid) });
                tallest = tallest.max(tip.y - fr.g);
            }
            // Side processes: lying on its side only the upper one shows.
            let sides: Vec<Vec3> = if on_side { vec![Vec3::Y] } else { vec![v3(fr.lat, 0.0), -v3(fr.lat, 0.0)] };
            if sec == 1 || (sec == 2 && u < 0.4) {
                for &l in &sides {
                    let a = c + l * 0.5 * r;
                    let tip = a + l * 1.3 * r + fr.dorsal * 0.3 * r;
                    list.push(Bone { flat: fr.dorsal, squash: 0.5, ..Bone::capsule(a, tip, (0.25 * r).max(MIN_R), (0.12 * r).max(MIN_R), sid) });
                }
            }
            if sec == 2 && u < 0.6 && on_side {
                let ch = 1.6 * r * (1.0 - u / 0.6);
                let a = c - fr.dorsal * 0.8 * r;
                list.push(Bone { flat: t3, squash: 0.45, ..Bone::capsule(a, a - fr.dorsal * ch + t3 * 0.35 * ch, (0.22 * r).max(MIN_R), (0.1 * r).max(MIN_R), sid) });
            }
            if sec == 0 && plan.cervical_ribs {
                for &l in &sides {
                    let a = c - fr.dorsal * 0.45 * r + l * 0.55 * r;
                    list.push(Bone::capsule(a, a + t3 * 1.7 * gap, (0.16 * r).max(MIN_R), (0.08 * r).max(MIN_R * 0.8), sid));
                }
            }
        }
    }

    // Ribs off the trunk vertebrae (not the last three, the hips). No two
    // alike, the deepest a third of the way back, many broken.
    let depth = sk.rib_height.unwrap_or(plan.rib_depth * len);
    let width = sk.rib_span.unwrap_or(plan.rib_width * len);
    let rib_r = (plan.rib_r * len).max(MIN_R);
    let nr = plan.n_trunk.saturating_sub(3);
    let mut rib_mid = Vec2::ZERO;
    for i in 0..nr {
        let (s, c, r) = cent[plan.n_neck + i];
        let u = if nr > 1 { i as f32 / (nr - 1) as f32 } else { 0.5 };
        let shape = (PI * (0.12 + 0.88 * u.powf(0.8))).sin().max(0.3);
        let fr = frame(s);
        let t3 = v3(fr.d, 0.0);
        let bl3 = v3(fr.bl, 0.0);
        if i == nr / 2 {
            rib_mid = fr.p;
        }
        let h = depth * (0.45 + 0.55 * shape) * dice.range(0.85, 1.05);
        let w = width * (0.55 + 0.45 * shape) * dice.range(0.9, 1.1);
        let sweep = h * dice.range(0.08, 0.2);
        let tilt = -v3(fr.lat, 0.0) * sk.lean * h;
        let rb = rib_r * dice.range(0.85, 1.15) * (0.7 + 0.3 * shape);
        let fallen = |list: &mut Vec<Bone>, dice: &mut Dice, from: Vec3, end: f32| {
            if end < 1.0 && dice.next() < 0.7 {
                let lie = Vec2::new(from.x, from.z) + fr.bl * h * dice.range(0.1, 0.4) + fr.d * dice.range(-0.3, 0.3) * h;
                let ang = dice.range(0.0, 6.28);
                let along = Vec3::new(ang.cos(), 0.0, ang.sin());
                let half = (1.0 - end) * h * 0.4;
                let cc = v3(lie, ground(lie) + tall(rb * 0.5).max(-rb * 0.3));
                list.push(Bone { flat: Vec3::Y, squash: 0.62, cut_a: true, cut_b: true, ..Bone::capsule(cc - along * half, cc + along * half, (rb * 0.5).max(MIN_R), (rb * 0.4).max(MIN_R), sid) });
            }
        };
        if on_side {
            // The upper rib: up from the spine, over, and down to the
            // ground on the belly side.
            if dice.next() > 0.05 {
                let base = c + Vec3::Y * 0.6 * r + bl3 * 0.3 * r;
                let tip_xz = fr.p + fr.bl * h * dice.range(0.85, 1.0) + fr.d * sweep;
                let tip = v3(tip_xz, ground(tip_xz) + rb * 0.5) + tilt;
                let c1 = base + Vec3::Y * w * 0.95 - bl3 * 0.05 * h;
                let c2 = v3(fr.p + fr.bl * h * 0.55, base.y + w) + t3 * sweep * 0.5 + tilt * 0.5;
                let broken = dice.next() < sk.broken;
                let end = if broken { dice.range(0.35, 0.8) } else { 1.0 };
                let stop = curved(list, [base, c1, c2, tip], end, rb, rb * 0.35, t3, 0.62, sid);
                tallest = tallest.max(stop.y - fr.g).max(base.y + w * 0.7 - fr.g);
                fallen(list, &mut dice, stop, end);
            }
            // The lower rib, pressed flat into the ground under it.
            if dice.next() > 0.2 {
                let base = c - Vec3::Y * 0.3 * r + bl3 * 0.3 * r;
                let tip_xz = fr.p + fr.bl * h * dice.range(0.8, 0.95) + fr.d * sweep * 1.4;
                let mid = fr.p + fr.bl * h * 0.5 - fr.d * h * 0.06;
                let lay = |q: Vec2| v3(q, ground(q) + tall(rb));
                curved(list, [base, lay(fr.p.lerp(mid, 0.5)), lay(mid), lay(tip_xz)], 1.0, rb * 0.9, rb * 0.35, t3, 0.5, sid);
            }
        } else {
            // Face down: hoops out either side and down to the ground.
            for sgn in [-1.0f32, 1.0] {
                if dice.next() < 0.06 {
                    continue;
                }
                let side = v3(fr.lat, 0.0) * sgn;
                let base = c + side * 0.9 * r + Vec3::Y * 0.4 * r;
                let tip_xz = fr.p + fr.lat * sgn * (w * 1.3 + 0.3 * h) + fr.d * sweep;
                let tip = v3(tip_xz, ground(tip_xz) + rb * 0.4);
                let c1 = base + Vec3::Y * h * 0.55 + side * w * 0.25;
                let c2 = base + Vec3::Y * h * 0.6 + side * w + t3 * sweep * 0.4 + tilt;
                let broken = dice.next() < sk.broken;
                let end = if broken { dice.range(0.35, 0.8) } else { 1.0 };
                let stop = curved(list, [base, c1, c2, tip], end, rb, rb * 0.35, t3, 0.62, sid);
                tallest = tallest.max(stop.y - fr.g).max(base.y + h * 0.45 - fr.g);
                fallen(list, &mut dice, stop, end);
            }
        }
        occupy.push((fr.p + fr.bl * h * 0.5 * if on_side { 1.0 } else { 0.0 }, h * 0.6 + w * 0.5));
    }

    // Limbs, lying on the ground: a chain of long bones with knobs at the
    // joints, then fingers or toes fanned out, clawed on a hunter. `bends`
    // turns each bone toward the tail (negative: toward the head) from the
    // last, starting from `out`.
    let limb_sink = sink * 0.6;
    let limb = |list: &mut Vec<Bone>, occupy: &mut Vec<(Vec2, f32)>, dice: &mut Dice, root: Vec2, out: Vec2, back: Vec2, bends: [f32; 3], bones3: [(f32, f32); 3], digits: (usize, f32), lift: f32| -> Vec2 {
        let mut q = root;
        let mut ang = 0.0;
        let mut lifted = lift;
        let mut last_dir = out;
        let lay = |q: Vec2, r: f32, extra: f32| v3(q, ground(q) + r * (1.0 - 2.0 * limb_sink) + extra);
        let mut r_end = MIN_R;
        for (k, &(l, r)) in bones3.iter().enumerate() {
            ang += bends[k];
            let dir = turn(out, back, ang);
            let (l, r) = (l * len, (r * len).max(MIN_R));
            let nq = q + dir * l;
            let next_lift = if k == 0 { 0.0 } else { lifted };
            let a = lay(q, r, lifted);
            let b = lay(nq, r, next_lift);
            list.push(Bone { flat: Vec3::Y, squash: 0.85, ..Bone::capsule(a, b, r * 0.85, r * 0.8, sid) });
            list.push(Bone::capsule(a, a, r * 1.2, r * 1.2, sid));
            if k == 2 {
                list.push(Bone::capsule(b, b, r * 1.05, r * 1.05, sid));
            }
            occupy.push((q.lerp(nq, 0.5), l * 0.5 + r));
            lifted = next_lift;
            q = nq;
            last_dir = dir;
            r_end = r;
        }
        let (count, dl) = digits;
        let side = Vec2::new(-last_dir.y, last_dir.x);
        for j in 0..count {
            let spread = (j as f32 - (count as f32 - 1.0) * 0.5) * 0.38;
            let dir = turn(last_dir, side, spread);
            let mut a = q + side * (j as f32 - (count as f32 - 1.0) * 0.5) * r_end * 0.9;
            let mut rr = (r_end * 0.45).max(MIN_R);
            let n_ph = if plan.claws { 3 } else { 2 };
            for ph in 0..n_ph {
                let l = dl * len * dice.range(0.85, 1.1) * if ph == 0 { 1.0 } else { 0.75 };
                let b = a + dir * l;
                list.push(Bone::capsule(lay(a, rr, 0.0), lay(b, rr, 0.0), rr, rr * 0.85, sid));
                a = b + dir * rr * 0.3;
                rr = (rr * 0.8).max(MIN_R);
            }
            if plan.claws || (j == 0 && !sauropod) || (sauropod && j < 2) {
                let l = dl * len * 0.8;
                list.push(Bone { flat: Vec3::Y, squash: 0.6, ..Bone::capsule(lay(a, rr, 0.0), lay(a + dir * l, 0.0, 0.0), rr * 1.1, 0.03, sid) });
            }
        }
        q
    };

    // Shoulders and hips, with each girdle's flat bones.
    let s_sh = s_neck + trunk_gap * 1.5;
    let s_hp = s_trunk - trunk_gap * 1.5;
    let sh = frame(s_sh);
    let hp = frame(s_hp);
    let (sc_l, sc_w) = ((plan.scapula.0 * len), (plan.scapula.1 * len).max(MIN_R));
    let (il_l, il_w) = ((plan.ilium.0 * len), (plan.ilium.1 * len).max(MIN_R));
    let plate = (rt * 0.6).max(MIN_R * 2.0) / sc_w.max(il_w);
    let squash = plate.clamp(0.15, 0.6);
    let outs: Vec<(Vec2, f32)> = if on_side { vec![(sh.bl, 0.0), (sh.bl, 1.0)] } else { vec![(sh.lat, 0.0), (-sh.lat, 0.0)] };
    for (k, &(out, upper)) in outs.iter().enumerate() {
        // The shoulder blade, slid off the ribs and lying on the ground.
        let joint = sh.p + out * rt * 1.6 + sh.d * upper * rt;
        let a = joint - sh.d * sc_l * 0.1;
        let b = joint + sh.d * sc_l * 0.9 + out * sc_l * 0.25 * if upper > 0.0 { 1.4 } else { 1.0 };
        let lay = |q: Vec2| v3(q, ground(q) + tall(sc_w * squash) + upper * sc_w * squash * 2.0);
        list.push(Bone { flat: Vec3::Y, squash, ..Bone::capsule(lay(a), lay(b), sc_w * 1.2, sc_w, plates) });
        occupy.push((a.lerp(b, 0.5), sc_l * 0.6));
        let bends = if upper > 0.0 { [-0.05, 0.4, 0.4] } else { [-0.35, 0.5, 0.3] };
        let foot = limb(list, &mut occupy, &mut dice, joint, out, sh.d, bends, plan.arm, plan.hand, upper * rt * 1.4);
        if k == 0 {
            places.push((format!("{}_forefoot", sk.name), foot));
        }
    }
    let outs: Vec<(Vec2, f32)> = if on_side { vec![(hp.bl, 0.0), (hp.bl, 1.0)] } else { vec![(hp.lat, 0.0), (-hp.lat, 0.0)] };
    let hip_c = cent[plan.n_neck + plan.n_trunk - 2].1;
    for (k, &(out, upper)) in outs.iter().enumerate() {
        let joint = hp.p + out * rt * 1.5 - hp.d * upper * rt;
        let bends = if upper > 0.0 { [0.6, -0.8, 0.5] } else { [0.25, -0.9, 0.6] };
        let foot = limb(list, &mut occupy, &mut dice, joint, out, hp.d, bends, plan.leg, plan.foot, upper * rt * 1.6);
        if k == 0 {
            places.push((format!("{}_foot", sk.name), foot));
        }
    }
    let t3 = v3(hp.d, 0.0);
    if on_side {
        // The upper hip blade lies flat over the hips; the pubis and the
        // ischium reach out along the ground toward the belly.
        let c = hip_c + Vec3::Y * (rt * 1.1);
        list.push(Bone { flat: Vec3::Y, squash, ..Bone::capsule(c - t3 * il_l * 0.5, c + t3 * il_l * 0.5, il_w, il_w * 0.8, plates) });
        let joint = hp.p + hp.bl * rt * 1.2;
        let lay = |q: Vec2, r: f32| v3(q, ground(q) + tall(r));
        let pr = (0.008 * len).max(MIN_R);
        let pubis = joint + turn(hp.bl, hp.d, -0.5) * (if sauropod { 0.065 } else { 0.09 }) * len;
        list.push(Bone::capsule(lay(joint, pr), lay(pubis, pr), pr, pr * 0.8, sid));
        if !sauropod {
            // The theropod's pubic boot.
            list.push(Bone { flat: Vec3::Y, squash: 0.6, ..Bone::capsule(lay(pubis - hp.d * pr * 2.0, pr * 2.0), lay(pubis + hp.d * pr * 2.0, pr * 2.0), pr * 2.0, pr * 1.6, sid) });
        }
        let ischium = joint + turn(hp.bl, hp.d, 0.6) * 0.055 * len;
        list.push(Bone::capsule(lay(joint, pr), lay(ischium, pr), pr, pr * 0.7, sid));
    } else {
        // Face down, both hip blades stand on edge beside the spine.
        for sgn in [-1.0f32, 1.0] {
            let c = hip_c + v3(hp.lat, 0.0) * sgn * rt * 1.3 + Vec3::Y * rt * 0.8;
            list.push(Bone { flat: v3(hp.lat, 0.0), squash, ..Bone::capsule(c - t3 * il_l * 0.5, c + t3 * il_l * 0.5, il_w, il_w * 0.8, plates) });
        }
    }
    occupy.push((hp.p, il_l * 0.6));

    // The skull, come off the neck and resting on its lower jaw at the head
    // of the path, snout away from the neck.
    let (o2, d0, _) = at(s_skull);
    let fwd = -d0;
    let f3 = v3(fwd, 0.0);
    let s2 = Vec2::new(-fwd.y, fwd.x);
    let s3 = v3(s2, 0.0);
    let skl = s_skull.max(if sauropod { 0.8 } else { 1.2 });
    let gc = ground(o2 + fwd * 0.5 * skl) - sk.skull_sink;
    let along = |w: f32| o2 + fwd * w * skl;
    let carve = |a: Vec3, b: Vec3, r: f32| Bone { carve: true, ..Bone::capsule(a, b, r, r, sid) };
    // Holes right through the skull, side to side.
    let hole = |c: Vec3, r: f32, half: f32| carve(c - s3 * half, c + s3 * half, r.max(MIN_R));
    let jr = (0.05 * skl).max(MIN_R);
    let jaw_back = 0.06;
    let chin = v3(along(0.95), gc + jr);
    for sgn in [-1.0f32, 1.0] {
        let a = v3(along(jaw_back) + s2 * sgn * 0.2 * skl, gc + jr * 1.4);
        let m = v3(along(0.5) + s2 * sgn * 0.12 * skl, gc + jr * 1.1);
        list.push(Bone { flat: s3, squash: 0.5, ..Bone::capsule(a, m, (0.09 * skl).max(jr), jr, sid) });
        list.push(Bone::capsule(m, chin + s3 * sgn * jr * 0.5, jr, jr * 0.8, sid));
        if tooth_mat != 0 {
            let count = if sauropod { 4 } else { 10 };
            for t in 0..count {
                if dice.next() < 0.2 {
                    continue;
                }
                let w = if sauropod { 0.8 + 0.15 * t as f32 / count as f32 } else { 0.35 + 0.58 * t as f32 / count as f32 };
                let q = v3(along(w) + s2 * sgn * 0.13 * skl * (1.0 - 0.6 * w), gc + jr * 1.8);
                let hgt = (if sauropod { 0.04 } else { 0.06 } * skl).max(0.25);
                list.push(Bone::capsule(q, q + Vec3::Y * hgt - f3 * 0.15 * hgt, (0.18 * hgt).max(MIN_R), 0.03, teeth_set));
            }
        }
    }
    let yb = gc + jr * 2.0 + 0.2 * skl;
    let (snout0, snout1, r0, r1) = if sauropod {
        // A short deep snout under a tall arch over the nostrils, and a
        // round braincase.
        list.push(Bone::capsule(v3(along(0.1), yb), v3(along(0.3), yb), 0.2 * skl, 0.2 * skl, sid));
        list.push(Bone::capsule(v3(along(0.3), yb + 0.2 * skl), v3(along(0.45), yb + 0.28 * skl), 0.13 * skl, 0.1 * skl, sid));
        list.push(hole(v3(along(0.28), yb + 0.08 * skl), 0.07 * skl, 0.3 * skl));
        list.push(hole(v3(along(0.42), yb + 0.26 * skl), 0.06 * skl, 0.2 * skl));
        (v3(along(0.35), yb), v3(along(0.95), gc + jr * 2.0 + 0.1 * skl), 0.17 * skl, 0.12 * skl)
    } else {
        // A broad braincase behind a long deep snout, the mouth agape, the
        // skull open in big windows: the eye, one behind it and a long one
        // in front.
        list.push(Bone { flat: Vec3::Y, squash: 0.9, ..Bone::capsule(v3(along(0.12), yb), v3(along(0.32), yb), 0.2 * skl, 0.19 * skl, sid) });
        list.push(hole(v3(along(0.31), yb + 0.08 * skl), 0.055 * skl, 0.3 * skl));
        list.push(hole(v3(along(0.17), yb), 0.05 * skl, 0.3 * skl));
        (v3(along(0.3), yb), v3(along(0.95), gc + jr * 2.0 + 0.17 * skl), 0.17 * skl, 0.075 * skl)
    };
    let snout_sq = if sauropod { 0.7 } else { 0.6 };
    let snout_flat = if sauropod { Vec3::Y } else { s3 };
    list.push(Bone { flat: snout_flat, squash: snout_sq, ..Bone::capsule(snout0, snout1, r0.max(MIN_R), r1.max(MIN_R), sid) });
    if !sauropod {
        for w in [0.45f32, 0.55] {
            let c = snout0.lerp(snout1, (w - 0.3) / 0.65);
            list.push(hole(c + Vec3::Y * 0.02 * skl, 0.06 * skl, 0.25 * skl));
        }
    }
    if tooth_mat != 0 {
        // Upper teeth hanging from the edges of the snout.
        let count = if sauropod { 5 } else { 12 };
        for sgn in [-1.0f32, 1.0] {
            for t in 0..count {
                if dice.next() < 0.18 {
                    continue;
                }
                let w = if sauropod { 0.75 + 0.22 * t as f32 / count as f32 } else { 0.1 + 0.85 * t as f32 / count as f32 };
                let c = snout0.lerp(snout1, w);
                let r = r0 + (r1 - r0) * w;
                let side_r = if sauropod { r } else { r * snout_sq };
                let vert_r = if sauropod { r * snout_sq } else { r };
                let base = c + s3 * sgn * side_r * 0.7 - Vec3::Y * vert_r * 0.7;
                let hgt = ((if sauropod { 0.05 } else { 0.08 }) * skl * (1.1 - 0.4 * w)).max(0.25);
                list.push(Bone { cut_b: dice.next() < 0.15, ..Bone::capsule(base, base - Vec3::Y * hgt - f3 * 0.2 * hgt, (0.18 * hgt).max(MIN_R), 0.03, teeth_set) });
            }
        }
    }
    let skull_c = along(0.5);
    occupy.push((skull_c, skl * 0.6));

    places.push((format!("{}_skull", sk.name), skull_c));
    places.push((format!("{}_neck", sk.name), at((s_skull + s_neck) * 0.5).0));
    places.push((format!("{}_ribs", sk.name), rib_mid));
    places.push((format!("{}_hips", sk.name), hp.p));
    places.push((format!("{}_tail", sk.name), at(len - 1.0).0));
    for k in 1..=9 {
        places.push((format!("{}_spine_{k}", sk.name), at(len * k as f32 / 10.0).0));
    }

    let count = bones.list.len() - first;
    *cx.state.get::<Bones>() = bones;
    let names = settle(cx, &occupy, &places);
    if let Some(poi) = &sk.poi {
        cx.pois.push(Poi { name: poi.clone(), pos: skull_c, cause });
    }
    format!(
        "skeleton {} ({} on its {}): {:.0} m long, {count} bones, rib cage {:.1} m deep, standing {:.1} m at most; places {}",
        sk.name,
        sk.kind,
        if on_side { "side" } else { "belly" },
        len,
        depth,
        tallest,
        names.join(", ")
    )
}
