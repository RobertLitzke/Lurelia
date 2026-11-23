//! `kind: "ribcage"`: a giant's ribs and nothing else, standing in two rows
//! either side of an open lane down `path`, the spine and everything else
//! long gone. Each rib rises from its head in the sand `gap` m out from the
//! lane's middle, bows outward, and curls back in over the lane, the two
//! rows never quite meeting overhead: a road or a path runs down the middle.

use super::{bent_bone, cubic, v3, Dice, Skeleton, Spine};
use crate::world::bones::{Bone, Bones};
use crate::world::chronicle::EventCtx;
use crate::world::fields::Poi;
use glam::{Vec2, Vec3};
use std::f32::consts::PI;

pub(super) fn lay(sk: &Skeleton, cx: &mut EventCtx, pts: &[Vec2]) -> String {
    let set = sk.bone_set(cx, 0.9);
    let cause = cx.cause;
    let mut dice = Dice(cx.seed ^ 0x71b5);
    let spine = Spine::new(&cx.f, pts, 30.0);
    let len = spine.len();
    let nr = sk.ribs.max(2);
    let spacing = len / nr as f32;
    let gap = sk.gap.unwrap_or(10.0);
    let rib_h = sk.rib_height.unwrap_or(0.4 * len);
    let rib_out = sk.rib_span.unwrap_or(0.4 * gap);
    let rib_r = sk.rib_radius.unwrap_or((0.0062 * len * 4.0).max(1.4));

    let mut bones = std::mem::take(cx.state.get::<Bones>());
    let sid = bones.add_set(set);
    let first = bones.list.len();
    let mut occupy: Vec<(Vec2, f32)> = Vec::new();
    let mut tallest: f32 = 0.0;
    let sea = cx.f.sea;

    for i in 0..nr {
        let u = if nr > 1 { i as f32 / (nr - 1) as f32 } else { 0.5 };
        // Tallest a third of the way along, lower toward both ends.
        let shape = (PI * (0.15 + 0.85 * u.powf(0.8))).sin().max(0.3);
        let s = (i as f32 + 0.5) * spacing;
        let (p, d, g) = spine.at(s);
        let lat = v3(Vec2::new(-d.y, d.x), 0.0);
        let t3 = v3(d, 0.0);
        let ph = rib_h * (0.55 + 0.45 * shape) * dice.range(0.9, 1.05);
        let pout = rib_out * (0.7 + 0.3 * shape) * dice.range(0.9, 1.1);
        let psweep = dice.range(-0.04, 0.1);
        let ptip = dice.range(0.15, 0.4);
        for sgn in [-1.0f32, 1.0] {
            if dice.next() < 0.04 {
                continue;
            }
            let h = ph * dice.range(0.96, 1.04);
            let out = pout * dice.range(0.95, 1.05);
            let sweep = psweep * h;
            let side = lat * sgn;
            let rb = rib_r * dice.range(0.88, 1.12) * (0.75 + 0.25 * shape);
            let mid = v3(p, g);
            let base = mid + side * gap * dice.range(0.97, 1.03) - Vec3::Y * rb * 0.6;
            let c1 = base + side * out * 0.9 + Vec3::Y * h * 0.32;
            let c2 = mid + side * (gap + out * 0.35) + Vec3::Y * h * 1.12 + t3 * sweep * 0.5;
            let c3 = mid + side * gap * ptip + Vec3::Y * h + t3 * sweep;
            let broken = dice.next() < sk.broken;
            let end = if broken { dice.range(0.55, 0.9) } else { 1.0 };
            let wob = (dice.range(0.0, 6.28), dice.range(0.002, 0.006) * h);
            // The curve, with a slow wobble so no two ribs match.
            let pt = |t: f32| cubic([base, c1, c2, c3], t) + t3 * (wob.1 * (t * 9.0 + wob.0).sin()) + side * (wob.1 * 0.6 * (t * 6.0 + wob.0 * 1.7).sin());
            // The rib's head, a knob half buried where it met the spine.
            bones.list.push(Bone::capsule(base - side * 0.4 * rb, base + side * 0.4 * rb + Vec3::Y * 0.4 * rb, rb * 1.3, rb * 1.1, sid));
            let prev = bent_bone(&mut bones.list, pt, 28, end, |w| (rb * (1.0 - 0.8 * w.powf(1.3))).max(0.5), t3, 0.62, sid);
            tallest = tallest.max(prev.y.max(pt(0.75 * end).y) - sea);
            occupy.push((Vec2::new(base.x, base.z) + Vec2::new(side.x, side.z) * out * 0.4, out * 0.6 + rb));
            if broken && dice.next() < 0.7 {
                // The piece that fell, lying outside the row.
                let lie = Vec2::new(base.x, base.z) + Vec2::new(side.x, side.z) * (out + dice.range(4.0, 14.0)) + d * dice.range(-6.0, 6.0);
                let gl = cx.f.height(lie.x, lie.y);
                let ang = dice.range(0.0, 6.28);
                let along = Vec3::new(ang.cos(), 0.0, ang.sin());
                let half = (1.0 - end) * h * dice.range(0.25, 0.45);
                let cc = v3(lie, gl + rb * 0.1);
                bones.list.push(Bone { flat: Vec3::Y, squash: 0.62, cut_a: true, cut_b: true, ..Bone::capsule(cc - along * half, cc + along * half, rb * 0.42, rb * 0.34, sid) });
            }
        }
    }

    let count = bones.list.len() - first;
    *cx.state.get::<Bones>() = bones;
    let places = vec![
        (format!("{}_ribs", sk.name), spine.at(len * 0.5).0),
        (format!("{}_ribs_start", sk.name), spine.at(0.0).0),
        (format!("{}_ribs_end", sk.name), spine.at(len).0),
    ];
    let names = super::settle(cx, &occupy, &places);
    if let Some(poi) = &sk.poi {
        cx.pois.push(Poi { name: poi.clone(), pos: spine.at(len * 0.5).0, cause });
    }
    format!(
        "skeleton {}: a ribcage {:.0} m long, {nr} pairs of ribs {:.0} m out from its middle, {count} bones, tallest {:.0} m over the sea; places {}",
        sk.name,
        len,
        gap,
        tallest,
        names.join(", ")
    )
}
