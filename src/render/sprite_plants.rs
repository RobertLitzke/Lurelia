//! Drawings for the big plant sprites, at the full texture size: a fern frond
//! (tinted, laid out along a SPRAY-like arching card) and a fireweed clump
//! (its own colours, on crossed cards). See mesh/sprites.rs `Rule::Herb`.

use super::sprite_tex::{blade, dot, leaf, paint, Img, Rng};
use crate::mesh::sprites::layer;
use std::f32::consts::{FRAC_PI_2, PI};

pub fn draw(l: u32, n: usize) -> Img {
    let mut img = vec![[0.0; 4]; n * n];
    let mut r = Rng(l.wrapping_mul(7919) + 29);
    match l {
        layer::FROND => frond(&mut img, &mut r, n as f32),
        layer::FIREWEED => fireweed(&mut img, &mut r, n as f32),
        layer::LUPINE => lupine(&mut img, &mut r, n as f32),
        layer::HEATHER => heather(&mut img, &mut r, n as f32),
        layer::GLASSBLOOM => glassbloom(&mut img, &mut r, n as f32),
        layer::SAGE => sage(&mut img, &mut r, n as f32),
        layer::TUFT_LONG => long_grass(&mut img, &mut r, n as f32),
        layer::RABBITBRUSH => rabbitbrush(&mut img, &mut r, n as f32),
        layer::TUFT_PATCH => grass_patch(&mut img, &mut r, n as f32),
        layer::HUCKLEBERRY => bush(&mut img, &mut r, n as f32, false),
        layer::THIMBLEBERRY => bush(&mut img, &mut r, n as f32, true),
        layer::AGAVE_STALK => agave_stalk(&mut img, &mut r, n as f32, false),
        layer::AGAVE_STALK_DRY => agave_stalk(&mut img, &mut r, n as f32, true),
        layer::YUCCA_STALK => yucca_stalk(&mut img, &mut r, n as f32),
        // Cattail leaves: tall, dark green straps, a few browning at the tip.
        layer::CATTAIL => reeds(&mut img, &mut r, n as f32, 0.5, 30, [0.07, 0.12, 0.035], [0.16, 0.13, 0.05]),
        layer::CATTAIL_HEAD => cattail_head(&mut img, &mut r, n as f32),
        // Sawgrass: a looser tussock, tawny olive going gold at the tips.
        layer::SAWGRASS => reeds(&mut img, &mut r, n as f32, 0.85, 40, [0.12, 0.13, 0.05], [0.24, 0.19, 0.07]),
        layer::PALMETTO => palmetto(&mut img, &mut r, n as f32),
        layer::CANE => cane(&mut img, &mut r, n as f32),
        layer::LILY_PAD => lily_pad(&mut img, &mut r, n as f32),
        _ => {}
    }
    img
}

/// A sword-fern frond seen from above: the stipe leaves the bottom centre, the
/// rachis runs to the top edge, and pinnae in near-opposite pairs sweep toward
/// the tip, longest a third of the way up. Each pinna is a row of small lobes
/// either side of its own midrib, so the outline is finely toothed. Grey, darker
/// toward the rachis and the base (77% grey is the fern block's colour).
fn frond(img: &mut Img, r: &mut Rng, s: f32) {
    let root = [s * 0.5, s];
    // The rachis sweeps to one side and back, a gentle parabola rather than a
    // ruler: a straight spine with even pinnae reads as a comb.
    let bow = r.range(0.07, 0.13) * s * if r.f() < 0.5 { 1.0 } else { -1.0 };
    let tip = [s * 0.5 + bow * 0.35, s * 0.03];
    let ctrl = [s * 0.5 + bow * 1.5, s * 0.52];
    let at = |t: f32| {
        let u = 1.0 - t;
        [u * u * root[0] + 2.0 * u * t * ctrl[0] + t * t * tip[0], u * u * root[1] + 2.0 * u * t * ctrl[1] + t * t * tip[1]]
    };
    // A little under the block colour, so a fern in sun doesn't turn lime.
    let warm = [0.86, 0.86, 0.76];
    let pairs = 24;
    let stipe = 0.14;
    for k in 0..pairs {
        // Uneven spacing, and here and there a pinna missing, so the two sides
        // never line up into rungs.
        let jitter = r.range(-0.35, 0.35);
        let t = stipe + (1.0 - stipe) * (k as f32 + 0.5 + jitter) / pairs as f32;
        let t = t.clamp(stipe, 0.995);
        // Lanceolate: widest around a third of the way up the blade.
        let b = (t - stipe) / (1.0 - stipe);
        let prof = (PI * b.powf(0.75)).sin().max(0.0).powf(0.7);
        let p0 = at(t);
        let p1 = at((t + 0.01).min(1.0));
        let up = (p1[1] - p0[1]).atan2(p1[0] - p0[0]);
        for side in [-1.0f32, 1.0] {
            if r.f() < 0.07 {
                continue;
            }
            let len = s * 0.47 * prof * r.range(0.82, 1.12) + 3.0;
            // Pinnae sweep forward more near the tip, and no two at one angle.
            let sweep = 1.25 - 0.45 * b + r.range(-0.16, 0.16);
            let ang = up + side * sweep;
            let q = [p0[0] + side * 1.5, p0[1] + r.range(-1.2, 1.2) + if side > 0.0 { 2.0 } else { 0.0 }];
            let e = [q[0] + ang.cos() * len, q[1] + ang.sin() * len];
            // Each pinna curves over toward the tip of the frond instead of
            // running straight out: the control point is pulled along the rachis.
            let curl = len * r.range(0.16, 0.3);
            let c = [q[0] + ang.cos() * len * 0.55 + up.cos() * curl, q[1] + ang.sin() * len * 0.55 + up.sin() * curl];
            // Alternate pinnae a little lighter and darker, so neighbours separate.
            let shade = (0.55 + 0.35 * b.min(0.8)) * if k % 2 == 0 { 1.0 } else { 0.86 } + r.range(-0.05, 0.05);
            blade(img, q, c, e, 3.0, 1.2, shade * 0.75, shade, warm);
            // Lobes along the pinna, smaller toward its tip.
            let lobes = ((len / 4.0) as usize).max(2);
            for j in 0..lobes {
                let u = (j as f32 + 0.6) / lobes as f32;
                let w = 1.0 - u;
                let pt = [
                    (1.0 - u) * (1.0 - u) * q[0] + 2.0 * u * (1.0 - u) * c[0] + u * u * e[0],
                    (1.0 - u) * (1.0 - u) * q[1] + 2.0 * u * (1.0 - u) * c[1] + u * u * e[1],
                ];
                // Lobes lean toward the pinna tip and stay short, leaving a gap
                // between one pinna and the next.
                let ll = (s * 0.022 * (0.45 + 0.55 * w)).max(2.5);
                let sh = shade * (0.85 + 0.25 * u);
                for ls in [-1.0f32, 1.0] {
                    leaf(img, pt, ang + ls * 0.6 + curl / len * 0.6, ll, ll * 0.6, sh, warm);
                }
            }
            // Pointed tip, bending the way the pinna curves.
            leaf(img, e, ang + curl / len * 0.8, (len * 0.18).max(3.0), 4.0, shade, warm);
        }
    }
    // Stipe and rachis on top, dark and a little warm.
    blade(img, root, ctrl, tip, 5.0, 1.5, 0.35, 0.6, [1.0, 0.95, 0.75]);
}

/// Fireweed: a clump of five to seven upright stems with narrow willow-like
/// leaves spiralling up them, and at the top a spike of magenta flowers that
/// opens from the bottom, buds tight and darker toward the nodding tip. Drawn
/// stretched sideways by 1/ASPECT so it looks right on a card ASPECT as wide as
/// it is tall (mesh/sprites.rs uses the same number).
/// Lupine: shorter, stouter spikes than fireweed, packed with violet pea
/// flowers paling to blue-grey buds at the tip, over fans of fingered leaves
/// low down.
fn lupine(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.75;
    let sx = 1.0 / ASPECT;
    let to = |x: f32, y: f32| [s * 0.5 + x * s * sx, s * (1.0 - y)];
    let stem_col = [0.06, 0.12, 0.05];
    let leaf_col = [0.08, 0.16, 0.07];
    let flower = [0.30, 0.13, 0.55];
    let bud = [0.30, 0.30, 0.42];
    // Fans of leaves round the foot: five or six fingers from one point.
    for _ in 0..7 {
        let c = to(r.range(-0.3, 0.3), r.range(0.08, 0.3));
        let base = r.range(-0.4, 0.4) - FRAC_PI_2;
        for q in 0..6 {
            let a = base + (q as f32 - 2.5) * 0.42;
            leaf(img, c, a, s * 0.09 * r.range(0.8, 1.1), s * 0.02, 1.0, leaf_col);
        }
    }
    for i in 0..5 {
        let x0 = r.range(-0.18, 0.18);
        let h = if i == 0 { 0.97 } else { r.range(0.6, 0.92) };
        let lean = r.range(-0.05, 0.05) + x0 * 0.3;
        let pos = |t: f32| (x0 + lean * t * t, h * t);
        let (ax, ay) = pos(0.0);
        let (cx, cy) = pos(0.5);
        let (bx, by) = pos(1.0);
        blade(img, to(ax, ay), to(cx, cy), to(bx, by), s * 0.016, s * 0.008, 1.0, 1.4, stem_col);
        // The spike: whorls of flowers from 45% up, packed close.
        let nf = 30;
        for k in 0..nf {
            let t = 0.45 + 0.55 * k as f32 / nf as f32;
            let (px, py) = pos(t);
            let open = 1.0 - k as f32 / nf as f32;
            let w = 0.035 * (0.45 + 0.55 * open);
            for side in [-1.0f32, 1.0] {
                let c = to(px + side * w * r.range(0.6, 1.0), py + r.range(-0.01, 0.01));
                let tone = r.range(0.8, 1.2);
                let col = if open > 0.25 { [flower[0] * tone, flower[1] * tone, flower[2] * tone] } else { bud };
                dot(img, c, s * (0.008 + 0.011 * open), col);
                // A pale banner on some of the open ones.
                if open > 0.4 && r.f() < 0.35 {
                    dot(img, [c[0], c[1] - s * 0.006], s * 0.005, [0.78, 0.74, 0.86]);
                }
            }
        }
    }
    let n = (img.len() as f32).sqrt() as usize;
    for y in 0..n {
        let t = y as f32 / n as f32;
        let k = 1.0 - 0.45 * ((t - 0.75) / 0.25).clamp(0.0, 1.0);
        for x in 0..n {
            let p = &mut img[y * n + x];
            for c in 0..3 {
                p[c] *= k;
            }
        }
    }
}

fn fireweed(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.6;
    let sx = 1.0 / ASPECT;
    // World-ish units: x in [-0.5, 0.5] * ASPECT of the height, y up from 0 to 1.
    let to = |x: f32, y: f32| [s * 0.5 + x * s * sx, s * (1.0 - y)];
    let stems = 6;
    let stem_col = [0.20, 0.07, 0.05];
    let leaf_col = [0.07, 0.17, 0.05];
    let flower = [0.62, 0.05, 0.30];
    let bud = [0.30, 0.03, 0.12];
    for i in 0..stems {
        let x0 = r.range(-0.05, 0.05);
        let h = if i == 0 { 0.97 } else { r.range(0.62, 0.95) };
        let lean = r.range(-0.07, 0.07) + x0 * 0.6;
        let pos = |t: f32| (x0 + lean * t * t, h * t);
        let (ax, ay) = pos(0.0);
        let (cx, cy) = pos(0.5);
        let (bx, by) = pos(1.0);
        // Leaves: lower two thirds, pointing out and up, shrinking toward the top.
        let nl = 16;
        for k in 0..nl {
            let t = 0.06 + 0.6 * k as f32 / nl as f32;
            let (px, py) = pos(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let len = (0.15 * (1.0 - t * 0.7)) * r.range(0.85, 1.1);
            let rise = r.range(0.35, 0.75);
            let (dx, dy) = (side * rise.cos() * len, rise.sin() * len);
            let p = to(px, py);
            let e = to(px + dx, py + dy);
            let ang = (e[1] - p[1]).atan2(e[0] - p[0]);
            let l = (e[0] - p[0]).hypot(e[1] - p[1]);
            let sh = r.range(0.7, 1.2) * (0.7 + 0.5 * t);
            leaf(img, p, ang, l, s * 0.035, sh, leaf_col);
        }
        blade(img, to(ax, ay), to(cx, cy), to(bx, by), s * 0.016, s * 0.007, 1.0, 1.4, stem_col);
        // The flower spike: open flowers on short stalks, then buds.
        let nf = 22;
        for k in 0..nf {
            let t = 0.62 + 0.38 * k as f32 / nf as f32;
            let (px, py) = pos(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let open = 1.0 - (k as f32 / nf as f32);
            let reach = 0.012 + 0.03 * open;
            let c = to(px + side * reach, py + 0.01 * open);
            if open > 0.35 {
                // Four petals round a pale centre, seen at a slant.
                let pr = s * (0.012 + 0.016 * open) * r.range(0.85, 1.15);
                let tone = r.range(0.85, 1.15);
                let col = [flower[0] * tone, flower[1] * tone, flower[2] * tone];
                for q in 0..4 {
                    let a = q as f32 * FRAC_PI_2 + r.range(-0.3, 0.3) + 0.4;
                    let off = [c[0] + a.cos() * pr * 0.55, c[1] + a.sin() * pr * 0.4];
                    dot(img, off, pr * 0.62, col);
                }
                dot(img, c, pr * 0.25, [0.85, 0.55, 0.65]);
                blade(img, to(px, py), [c[0], c[1] + 2.0], c, 1.8, 1.2, 1.0, 1.0, stem_col);
            } else {
                let br = s * (0.006 + 0.01 * open / 0.35);
                dot(img, c, br, bud);
            }
        }
        // A nodding tip.
        let (tx, ty) = pos(1.0);
        dot(img, to(tx + lean * 0.05, ty + 0.005), s * 0.006, bud);
    }
    // Darken the base, so the clump sits in the grass instead of floating on it.
    let n = (img.len() as f32).sqrt() as usize;
    for y in 0..n {
        let t = y as f32 / n as f32;
        let k = 1.0 - 0.45 * ((t - 0.75) / 0.25).clamp(0.0, 1.0);
        for x in 0..n {
            let p = &mut img[y * n + x];
            for c in 0..3 {
                p[c] *= k;
            }
        }
    }
}

/// A water lily's pad seen from above: a round leaf with a wedge notch cut to
/// its middle, a raised rim, and veins running out from the stalk. Tinted by
/// the block it grows from, so a world sets the green in its material; the
/// shading here is the relief. Drawn on a square card (kind FLOAT).
fn lily_pad(img: &mut Img, r: &mut Rng, s: f32) {
    let c = [s * 0.5, s * 0.5];
    // Not quite round, and the notch points a different way on each pad.
    let notch = r.range(0.0, std::f32::consts::TAU);
    let half = r.range(0.10, 0.17);
    let wob = r.range(0.02, 0.05);
    let ph = r.range(0.0, std::f32::consts::TAU);
    let mottle = r.range(0.0, 97.0);
    let rad = |a: f32| s * 0.46 * (1.0 + wob * (a * 3.0 + ph).sin());
    // The leaf: coverage inside the wobbled circle, minus the notch wedge.
    paint(img, 0.0, 0.0, s, s, |x, y| {
        let (dx, dy) = (x - c[0], y - c[1]);
        let d = (dx * dx + dy * dy).sqrt();
        let a = dy.atan2(dx);
        let rr = rad(a);
        if d > rr {
            return None;
        }
        // The wedge, wide at the rim and closing to the middle.
        let off = (a - notch + PI).rem_euclid(std::f32::consts::TAU) - PI;
        if off.abs() < half * (d / rr) {
            return None;
        }
        let cov = (rr - d).min(1.0);
        // Paler along the veins and at the rim, darker in the hollow middle.
        let veins = ((a * 9.0 + ph).sin() * 0.5 + 0.5).powf(6.0);
        let t = d / rr;
        let grain = ((x * 0.7 + mottle).sin() * (y * 0.9 + mottle).cos()) * 0.03;
        let shade = 0.46 + 0.18 * t + 0.16 * veins * smooth_from(t) + grain;
        Some((cov, [shade * 0.92, shade, shade * 0.6]))
    });
    // A darker rim all round, so neighbouring pads read apart on the water.
    paint(img, 0.0, 0.0, s, s, |x, y| {
        let (dx, dy) = (x - c[0], y - c[1]);
        let d = (dx * dx + dy * dy).sqrt();
        let rr = rad(dy.atan2(dx));
        let e = (d - (rr - 2.5)).max(0.0) / 2.5;
        (d <= rr && e > 0.0).then(|| (e * 0.5, [0.26, 0.30, 0.16]))
    });
}

/// How far out from the middle a vein shows: nothing at the stalk, full at
/// the rim.
fn smooth_from(t: f32) -> f32 {
    (t * 1.6).clamp(0.0, 1.0)
}

/// A water lily's bloom seen from above: a ring of pointed petals round a
/// gold heart, in its own colours (untinted).
pub(super) fn lily_flower(img: &mut Img, r: &mut Rng, s: f32) {
    let c = [s * 0.5, s * 0.5];
    let turn = r.range(0.0, std::f32::consts::TAU);
    let pink = r.f() < 0.45;
    let petal = if pink { [0.78, 0.42, 0.52] } else { [0.88, 0.86, 0.80] };
    for ring in 0..3 {
        let n = 9 - ring;
        let len = s * (0.44 - 0.09 * ring as f32);
        let shade = 0.72 + 0.14 * ring as f32;
        for k in 0..n {
            let a = turn + ring as f32 * 0.35 + k as f32 * std::f32::consts::TAU / n as f32;
            let p = [c[0] + a.cos() * len * 0.42, c[1] + a.sin() * len * 0.42];
            leaf(img, p, a, len, len * 0.34, shade, petal);
        }
    }
    dot(img, c, s * 0.10, [0.92, 0.76, 0.22]);
    dot(img, c, s * 0.05, [1.0, 0.92, 0.40]);
}

/// Darken the bottom quarter so a clump sits in the ground.
fn darken_base(img: &mut Img, amount: f32) {
    let n = (img.len() as f32).sqrt() as usize;
    for y in 0..n {
        let k = 1.0 - amount * ((y as f32 / n as f32 - 0.75) / 0.25).clamp(0.0, 1.0);
        for x in 0..n {
            for c in 0..3 {
                img[y * n + x][c] *= k;
            }
        }
    }
}

/// Long grass, 1.1 to 1.6 m: a tinted clump of long blades that rise steeply
/// and arch over toward their tips, with a few seed stalks above. Shown 0.75 as
/// wide as tall (mesh/sprites.rs), so drawn stretched by 1/0.75. Coordinates
/// are in heights: x from the middle (the card is +-0.375 wide), y up.
fn long_grass(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.75;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let warm = [1.0, 1.0, 0.9];
    let n = 46;
    for i in 0..n {
        let x0 = (r.f() + r.f() - 1.0) * 0.16;
        let edge = x0.abs() / 0.16;
        let h = r.range(0.55, 0.97) * (1.0 - 0.3 * edge);
        // Outward from the middle, plus a sway; `arch` is how far the tip falls over.
        let lean = x0 / 0.16 * 0.1 + r.range(-0.1, 0.1);
        let arch = r.f();
        let tipx = (x0 + lean + lean.signum() * arch * 0.1).clamp(-0.35, 0.35);
        let tipy = h * (1.0 - 0.2 * arch);
        let ctrl = (x0 + (tipx - x0) * 0.15, (tipy + 0.18 * arch * h).min(0.99));
        let back = if i < n / 2 { 0.82 } else { 1.0 };
        let sh = r.range(0.88, 1.0) * back;
        blade(img, to(x0, -0.005), to(ctrl.0, ctrl.1), to(tipx, tipy), s * 0.016, s * 0.004, 0.42 * sh, sh, warm);
    }
    // Seed stalks standing clear of the blades, each with a slim drooping plume.
    for _ in 0..5 {
        let x0 = r.range(-0.06, 0.06);
        let h = r.range(0.9, 0.98);
        let tipx = x0 + r.range(-0.1, 0.1);
        let (a, c, b) = (to(x0, -0.005), to(x0 + (tipx - x0) * 0.3, h * 0.5), to(tipx, h));
        blade(img, a, c, b, s * 0.007, s * 0.005, 0.5, 0.95, [1.0, 0.97, 0.82]);
        let ang = (c[1] - b[1]).atan2(c[0] - b[0]);
        leaf(img, b, ang, s * 0.11, s * 0.022, 1.0, [1.0, 0.95, 0.78]);
    }
}

/// A far grass patch: a 2 m strip of grass on one card (mesh/sprites.rs
/// TUFT_PATCH), seen only from 26 m out, so it is about coverage and an
/// uneven top edge, not single blades. The card is about four times as wide as
/// tall, so the drawing is squeezed by 1/4. Blades get shorter toward the ends
/// so the card has no square corners, and the back half is darker.
fn grass_patch(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 2.0;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let warm = [1.0, 1.0, 0.9];
    // Blades fill the card's own width, whatever aspect it is drawn at.
    const HALF: f32 = ASPECT * 0.48;
    let n = 110;
    for i in 0..n {
        let x0 = r.range(-HALF, HALF);
        let end = (x0.abs() / HALF).powi(3);
        // Clumps: the top edge rises and falls along the strip.
        let swell = 0.75 + 0.25 * (x0 * 3.1 + 1.3).sin() * (x0 * 1.7).cos();
        let h = r.range(0.45, 0.98) * swell * (1.0 - 0.7 * end);
        let lean = r.range(-0.12, 0.12) - x0 * 0.03;
        let (tipx, tipy) = (x0 + lean, h);
        let ctrl = (x0 + lean * 0.3, h * 0.55);
        let back = if i < n / 2 { 0.8 } else { 1.0 };
        let sh = r.range(0.86, 1.0) * back;
        blade(img, to(x0, -0.005), to(ctrl.0, ctrl.1), to(tipx, tipy), s * 0.012, s * 0.003, 0.45 * sh, sh, warm);
    }
}

/// Heather: a low, wide clump of wiry stems fanning out from the root, clothed
/// in tiny dark leaves, the upper half of each stem a spike of small pink-mauve
/// bells. Shown 1.4 as wide as tall (mesh/sprites.rs), so drawn squeezed by 1/1.4.
fn heather(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 1.4;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let stem = [0.13, 0.06, 0.04];
    let foliage = [0.05, 0.11, 0.04];
    let n = 26;
    // Back stems first, darker, so the clump has depth.
    for i in 0..n {
        let back = i < n / 2;
        let x0 = r.range(-0.12, 0.12);
        let spread = r.range(-1.0, 1.0);
        let h = r.range(0.5, 1.0) * (1.0 - 0.35 * spread.abs());
        let tipx = x0 + spread * 0.5;
        let pos = |t: f32| (x0 + (tipx - x0) * t.powf(0.7), h * t);
        let dim = if back { 0.7 } else { 1.0 };
        let (ax, ay) = pos(0.0);
        let (cx, cy) = pos(0.5);
        let (bx, by) = pos(1.0);
        blade(img, to(ax, ay), to(cx, cy), to(bx, by), s * 0.010, s * 0.005, 0.8 * dim, 1.2 * dim, stem);
        // Tiny leaves along the whole stem: short dark dashes.
        let leaves = 16;
        for k in 0..leaves {
            let t = 0.1 + 0.85 * k as f32 / leaves as f32;
            let (px, py) = pos(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let p = to(px, py);
            let ang = -std::f32::consts::FRAC_PI_2 + side * r.range(0.5, 1.0);
            leaf(img, p, ang, s * 0.03, s * 0.014, r.range(0.8, 1.3) * dim, foliage);
        }
        // Bells on the upper half, crowded and smaller toward the tip.
        let bells = 14;
        for k in 0..bells {
            let t = 0.45 + 0.55 * k as f32 / bells as f32;
            let (px, py) = pos(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let c = to(px + side * 0.012, py);
            let tone = r.range(0.75, 1.2) * dim;
            let pink = [0.48 * tone, 0.11 * tone, 0.36 * tone];
            dot(img, c, s * (0.012 - 0.005 * (t - 0.45) / 0.55), pink);
        }
    }
    darken_base(img, 0.5);
}

/// Glassbloom (an invented plant of the Glassfall strewn field): a fan of hard,
/// glassy blades from the root, each faceted down its midline, the lit side
/// pale lilac with a white edge, the other side deep violet. A few blades end in
/// a pale star of facets, the bloom. Own colours, shown 0.9 as wide as tall.
fn glassbloom(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.9;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let light = [0.72, 0.64, 0.92];
    let dark = [0.20, 0.13, 0.42];
    let edge = [0.95, 0.94, 1.0];
    let n = 11;
    let mut tips = Vec::new();
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32 - 0.5;
        let lean = f * 1.5 + r.range(-0.12, 0.12);
        let h = r.range(0.55, 0.98) * (1.0 - 0.3 * f.abs());
        let base = to(r.range(-0.03, 0.03), 0.0);
        let tip = to(lean * 0.5, h);
        let (dx, dy) = (tip[0] - base[0], tip[1] - base[1]);
        let len = dx.hypot(dy);
        let (ux, uy) = (dx / len, dy / len);
        let w = s * r.range(0.04, 0.06);
        let lit_left = lean > 0.0;
        let pad = w + 2.0;
        let (x0, x1) = (base[0].min(tip[0]) - pad, base[0].max(tip[0]) + pad);
        let (y0, y1) = (base[1].min(tip[1]) - pad, base[1].max(tip[1]) + pad);
        paint(img, x0, y0, x1, y1, |x, y| {
            let (rx, ry) = (x - base[0], y - base[1]);
            let along = rx * ux + ry * uy;
            let t = -rx * uy + ry * ux;
            if along < 0.0 || along > len + 1.0 {
                return None;
            }
            // Straight-sided blade, widest a fifth of the way up, to a point.
            let a = along / len;
            let half = w * 0.5 * if a < 0.2 { 0.5 + 2.5 * a } else { (1.0 - a) / 0.8 };
            let cov = (half - t.abs() + 0.5).clamp(0.0, 1.0);
            if cov <= 0.0 {
                return None;
            }
            let lit = (t < 0.0) == lit_left;
            let mut c = if lit { light } else { dark };
            // A bright line along the lit edge, and the facet ridge.
            if lit && half - t.abs() < 1.6 {
                c = edge;
            }
            if t.abs() < 0.8 {
                c = [(light[0] + dark[0]) * 0.6, (light[1] + dark[1]) * 0.6, (light[2] + dark[2]) * 0.6];
            }
            let k = 0.6 + 0.4 * a;
            Some((cov, [c[0] * k, c[1] * k, c[2] * k]))
        });
        if r.f() < 0.45 {
            tips.push(tip);
        }
    }
    for t in tips {
        let rad = s * r.range(0.025, 0.04);
        for k in 0..5 {
            let a = k as f32 / 5.0 * std::f32::consts::TAU + r.range(0.0, 0.4);
            leaf(img, t, a, rad, rad * 0.5, 1.0, [0.86, 0.95, 1.0]);
        }
        dot(img, t, rad * 0.25, [1.0, 1.0, 0.95]);
    }
    darken_base(img, 0.35);
}

/// Sagebrush leaves: a billboard of small wedge-shaped leaves, each widening to
/// a three-toothed tip, on grey twigs, pointing every way from the middle and
/// leaving a ragged outline. Grey, tinted by the sage block.
fn sage(img: &mut Img, r: &mut Rng, s: f32) {
    // A sprig off the crown, not a rosette: a woody stem up from the bottom of
    // the card, two or three branches off it, and the little three-toothed
    // leaves massed in bunches at the ends and along them, overlapping into
    // fuller clumps. The billboard is rolled at random, so the stem reads as a
    // twig leaving the bush in any direction.
    let wood = [1.0, 0.95, 0.85];
    let root = [s * 0.5, s * 0.99];
    let bow = r.range(-0.1, 0.1) * s;
    let tip = [s * 0.5 + bow, s * 0.26];
    let ctrl = [s * 0.5 + bow * 0.4, s * 0.62];
    let at = |t: f32| {
        let u = 1.0 - t;
        [u * u * root[0] + 2.0 * u * t * ctrl[0] + t * t * tip[0], u * u * root[1] + 2.0 * u * t * ctrl[1] + t * t * tip[1]]
    };
    blade(img, root, ctrl, tip, s * 0.022, s * 0.01, 0.4, 0.5, wood);
    // Where the leaf bunches sit: the stem tip, the end of each branch, and a
    // couple of points along the stem.
    let mut bunches = vec![(tip, 0.95f32), (at(0.55), 0.8), (at(0.3), 0.66)];
    for (t0, side) in [(0.38f32, -1.0f32), (0.62, 1.0)] {
        let p0 = at(t0);
        let len = s * r.range(0.2, 0.3);
        let a = -FRAC_PI_2 + side * r.range(0.55, 0.95);
        let p1 = [p0[0] + a.cos() * len, p0[1] + a.sin() * len];
        blade(img, p0, [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5], p1, s * 0.016, s * 0.008, 0.38, 0.48, wood);
        bunches.push((p1, if side < 0.0 { 0.72 } else { 0.9 }));
        bunches.push(([(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5], 0.6));
    }
    // Darkest bunches first, so the near ones overlap them.
    bunches.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    for (c, step) in bunches {
        for _ in 0..r.range(7.0, 10.99) as usize {
            let p = [c[0] + r.range(-0.045, 0.045) * s, c[1] + r.range(-0.045, 0.045) * s];
            let dir = r.range(0.0, std::f32::consts::TAU);
            let len = s * r.range(0.1, 0.15);
            // None near white, which the tint would turn to frost.
            let sh = step * r.range(0.82, 0.95);
            leaf(img, p, dir, len, len * 0.42, sh, [1.0, 1.0, 0.94]);
            // Three teeth across the broad tip.
            let t = [p[0] + dir.cos() * len * 0.85, p[1] + dir.sin() * len * 0.85];
            let side = [-dir.sin(), dir.cos()];
            for k in [-1.0f32, 0.0, 1.0] {
                let q = [t[0] + side[0] * k * len * 0.16, t[1] + side[1] * k * len * 0.16];
                dot(img, q, len * 0.09, [sh, sh, sh * 0.94]);
            }
        }
    }
}

/// Rabbitbrush: a rounded bush of thirty-odd thin upright stems fanning out
/// from the root, grey-green with narrow leaves, each topped by a flat head of
/// small yellow flowers, so the top of the bush is a dome of yellow. Its own
/// colours. Shown 1.15 as wide as tall (mesh/sprites.rs).
fn rabbitbrush(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 1.15;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let stem = [0.16, 0.18, 0.10];
    let leaf_col = [0.20, 0.24, 0.12];
    let gold = [0.78, 0.52, 0.05];
    let deep = [0.48, 0.30, 0.03];
    let n = 34;
    let mut tops = Vec::new();
    for i in 0..n {
        // Spread across the dome: outer stems lower and leaning further out.
        let u = (i as f32 + r.f()) / n as f32 * 2.0 - 1.0;
        let x0 = u * 0.06;
        let tipx = u * 0.42 + r.range(-0.04, 0.04);
        let tipy = (0.95 - 0.38 * u * u) * r.range(0.88, 1.0);
        let ctrl = (x0 + (tipx - x0) * 0.35, tipy * 0.55);
        let back = if i % 2 == 0 { 0.8 } else { 1.0 };
        blade(img, to(x0, -0.005), to(ctrl.0, ctrl.1), to(tipx, tipy), s * 0.012, s * 0.006, 0.7 * back, 1.1 * back, stem);
        // Narrow leaves up the lower part of the stem.
        for k in 0..5 {
            let t = 0.2 + 0.12 * k as f32;
            let px = x0 + (tipx - x0) * t * t;
            let py = tipy * t;
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let p = to(px, py);
            let ang = -std::f32::consts::FRAC_PI_2 + side * r.range(0.4, 0.8);
            leaf(img, p, ang, s * r.range(0.05, 0.08), s * 0.012, r.range(0.7, 1.1) * back, leaf_col);
        }
        tops.push((tipx, tipy, back));
    }
    // Flower heads last, so the yellow sits on top of the stems.
    for (x, y, back) in tops {
        for _ in 0..7 {
            let p = to(x + r.range(-0.035, 0.035), y + r.range(-0.03, 0.025));
            let tone = r.range(0.8, 1.15) * back;
            let col = if r.f() < 0.3 { deep } else { [gold[0] * tone, gold[1] * tone, gold[2] * tone] };
            dot(img, p, s * r.range(0.011, 0.018), col);
        }
    }
    darken_base(img, 0.4);
}

/// A forest-floor bush: arching stems from the root, leafy along their upper
/// two thirds, darker inside and at the base. Huckleberry (`broad` false) has
/// many small oval leaves and a few dark berries, shown 1.25 as wide as tall;
/// thimbleberry has fewer, big, lobed leaves held flat-ish, shown 1.2 as wide.
/// Real colours, kept near the forest canopy greens so a bush in shade doesn't
/// glow.
fn bush(img: &mut Img, r: &mut Rng, s: f32, broad: bool) {
    let aspect = if broad { 1.2 } else { 1.25 };
    let to = |x: f32, y: f32| [s * 0.5 + x * s / aspect, s * (1.0 - y)];
    let stem = [0.20, 0.13, 0.08];
    let green = if broad { [0.20, 0.34, 0.09] } else { [0.15, 0.30, 0.10] };
    let n = if broad { 11 } else { 16 };
    for i in 0..n {
        let back = i < n / 2;
        let dim = if back { 0.68 } else { 1.0 };
        let u = (i as f32 + r.f()) / n as f32 * 2.0 - 1.0;
        let x0 = u * 0.08;
        let tipx = u * 0.46 + r.range(-0.05, 0.05);
        let tipy = (0.92 - 0.35 * u * u) * r.range(0.8, 1.0);
        // Arching: the control point sits high, so the stem bows outward.
        let ctrl = (x0 + (tipx - x0) * 0.3, tipy * 1.05);
        let pos = |t: f32| {
            let v = 1.0 - t;
            (v * v * x0 + 2.0 * v * t * ctrl.0 + t * t * tipx, 2.0 * v * t * ctrl.1 + t * t * tipy)
        };
        blade(img, to(x0, -0.005), to(ctrl.0, ctrl.1), to(tipx, tipy), s * 0.011, s * 0.005, 0.6 * dim, 1.0 * dim, stem);
        let leaves = if broad { 4 } else { 11 };
        for k in 0..leaves {
            let t = 0.35 + 0.65 * (k as f32 + r.f() * 0.5) / leaves as f32;
            let (px, py) = pos(t);
            let side = if k % 2 == 0 { 1.0 } else { -1.0 };
            let p = to(px, py);
            let tone = r.range(0.8, 1.15) * dim;
            let col = [green[0] * tone, green[1] * tone, green[2] * tone];
            if broad {
                // A lobed leaf: three overlapping leaves fanned from one point.
                let size = s * r.range(0.07, 0.1);
                let base = -std::f32::consts::FRAC_PI_2 + side * r.range(0.5, 1.2);
                for (da, f) in [(-0.6f32, 0.75f32), (0.0, 1.0), (0.6, 0.75)] {
                    leaf(img, p, base + da, size * f, size * f * 0.8, 1.0, col);
                }
            } else {
                let ang = -std::f32::consts::FRAC_PI_2 + side * r.range(0.6, 1.3);
                leaf(img, p, ang, s * r.range(0.035, 0.05), s * 0.026, 1.0, col);
            }
        }
        if !broad && !back && r.f() < 0.5 {
            let (px, py) = pos(r.range(0.5, 0.85));
            dot(img, to(px + r.range(-0.02, 0.02), py - 0.03), s * 0.009, [0.12, 0.10, 0.22]);
        }
    }
    darken_base(img, 0.55);
}

/// An agave's flower stalk: a stout pole rising from the rosette, its top
/// half carrying short side branches that turn up, each ending in a flat
/// cluster of yellow flowers, so the top reads as a candelabra. `dry` is the
/// stalk a year on: grey-brown, with dark seed pods where the flowers were.
/// Shown 0.6 as wide as tall (mesh/sprites.rs), stiff in the wind.
fn agave_stalk(img: &mut Img, r: &mut Rng, s: f32, dry: bool) {
    const ASPECT: f32 = 0.6;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let (pole, twig) = if dry {
        ([0.17, 0.13, 0.09], [0.14, 0.10, 0.07])
    } else {
        ([0.12, 0.12, 0.05], [0.10, 0.11, 0.04])
    };
    let (bloom, deep) = if dry {
        ([0.16, 0.10, 0.05], [0.08, 0.05, 0.03])
    } else {
        ([0.80, 0.56, 0.06], [0.42, 0.28, 0.03])
    };
    let lean = r.range(-0.03, 0.03);
    let pos = |t: f32| (lean * t * t, 0.97 * t);
    let (ax, ay) = pos(0.0);
    let (cx, cy) = pos(0.5);
    let (bx, by) = pos(1.0);
    // About 12 cm at the base on a 4 m stalk (the card is 0.6 as wide).
    blade(img, to(ax, ay), to(cx, cy), to(bx, by), s * 0.05, s * 0.016, 0.8, 1.15, pole);
    // Side branches over the top 45%, alternating, shorter toward the tip.
    let n = 9;
    for k in 0..n {
        let t = 0.55 + 0.42 * k as f32 / n as f32;
        let (px, py) = pos(t);
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let len = (0.26 - 0.17 * (t - 0.55) / 0.42) * r.range(0.85, 1.1);
        let e = (px + side * len, py + len * r.range(0.35, 0.55));
        let c = (px + side * len * 0.6, py + len * 0.05);
        blade(img, to(px, py), to(c.0, c.1), to(e.0, e.1), s * 0.02, s * 0.01, 0.8, 1.05, twig);
        // The cluster: a dense, flat-topped mound of flowers on the branch
        // end, darker underneath.
        let w = 0.04 + 0.05 * len / 0.26;
        for j in 0..34 {
            let dx = r.range(-1.0, 1.0) * w;
            let top = 1.0 - (dx / w).powi(2);
            let dy = r.range(-0.3, 0.6 * top + 0.1) * w;
            let tone = r.range(0.8, 1.15) * if j < 10 { 0.75 } else { 1.0 };
            let col = if r.f() < 0.25 { deep } else { [bloom[0] * tone, bloom[1] * tone, bloom[2] * tone] };
            dot(img, to(e.0 + dx, e.1 + dy), s * r.range(0.011, 0.018), col);
        }
    }
    // A small cluster at the very top.
    for _ in 0..8 {
        let p = to(bx + r.range(-0.03, 0.03), by + r.range(-0.03, 0.01));
        dot(img, p, s * r.range(0.01, 0.015), if r.f() < 0.3 { deep } else { bloom });
    }
}

/// A yucca's flower spike: a green-tan stalk, bare for its lower third, then
/// crowded with nodding creamy-white bells, a few touched with purple.
/// Shown 0.32 as wide as tall, stiff in the wind.
fn yucca_stalk(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.32;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let stem = [0.12, 0.13, 0.06];
    let cream = [0.86, 0.84, 0.68];
    let blush = [0.70, 0.56, 0.56];
    let lean = r.range(-0.02, 0.02);
    let pos = |t: f32| (lean * t * t, 0.96 * t);
    let (ax, ay) = pos(0.0);
    let (cx, cy) = pos(0.5);
    let (bx, by) = pos(1.0);
    // About 3 cm thick: the card is a third as wide as tall, so widths are drawn wide.
    blade(img, to(ax, ay), to(cx, cy), to(bx, by), s * 0.06, s * 0.03, 0.8, 1.1, stem);
    // A panicle: short branches leave the stalk, angled up and out, and the
    // bells hang in a row down each one. Pedicels straight off the stalk make
    // ladder rungs, which is what this used to read as.
    let arms = 13;
    for k in 0..arms {
        let t = 0.34 + 0.62 * (k as f32 + r.range(0.1, 0.9)) / arms as f32;
        let (px, py) = pos(t);
        // Longest low down, shortest at the tip.
        let up = (t - 0.34) / 0.62;
        let reach = 0.115 * (1.0 - 0.72 * up) * r.range(0.8, 1.1);
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let rise = 0.055 * (1.0 - 0.5 * up) * r.range(0.7, 1.2);
        let ex = px + side * reach;
        let ey = py + rise;
        blade(img, to(px, py), to(px + side * reach * 0.5, py + rise * 0.75), to(ex, ey), s * 0.014, s * 0.008, 0.7, 1.0, stem);
        let bells = r.range(2.0, 4.99) as usize;
        for j in 0..bells {
            let u = (j as f32 + r.range(0.2, 0.8)) / bells as f32;
            // Along the branch, each bell on its own short stalk hanging down.
            let bxp = px + side * reach * u;
            let byp = py + rise * u * (2.0 - u);
            let drop = r.range(0.02, 0.045);
            let c = to(bxp + side * r.range(-0.01, 0.02), byp - drop);
            blade(img, to(bxp, byp), to(bxp, byp - drop * 0.5), c, s * 0.008, s * 0.005, 0.7, 0.95, stem);
            let tone = r.range(0.78, 1.05);
            let col = if r.f() < 0.15 { blush } else { cream };
            let ang = FRAC_PI_2 + r.range(-0.3, 0.3);
            leaf(img, c, ang, s * r.range(0.03, 0.042), s * r.range(0.05, 0.07), tone, col);
        }
    }
    darken_base(img, 0.3);
}

/// A clump of strap-like leaves standing nearly straight up from the water,
/// the outer ones leaning out and a few bending over near the top; each runs
/// from `base` colour to `tip` colour over its top quarter. Drawn `aspect` as
/// wide as tall (mesh/sprites.rs gives the card that shape).
fn reeds(img: &mut Img, r: &mut Rng, s: f32, aspect: f32, n: usize, base: [f32; 3], tip: [f32; 3]) {
    let to = |x: f32, y: f32| [s * 0.5 + x * s / aspect, s * (1.0 - y)];
    for i in 0..n {
        let x0 = (r.f() + r.f() - 1.0) * 0.07 * aspect / 0.5;
        let out = x0 / (0.07 * aspect / 0.5);
        let h = r.range(0.6, 0.98) * (1.0 - 0.25 * out.abs());
        let lean = out * 0.08 + r.range(-0.04, 0.04);
        let bend = if r.f() < 0.25 { r.range(0.05, 0.14) } else { 0.0 };
        let tipx = (x0 + lean + lean.signum() * bend).clamp(-0.48 * aspect, 0.48 * aspect);
        let tipy = h - bend * 0.8;
        let ctrl = (x0 + (tipx - x0) * 0.25, (h * 0.8).min(0.99));
        let back = if i < n / 3 { 0.75 } else { 1.0 };
        let sh = r.range(0.85, 1.05) * back;
        let brown = r.f() < 0.3;
        // The lower part in the base colour, the top in the tip colour when browning.
        let mid = (x0 + (tipx - x0) * 0.6, tipy * 0.78);
        blade(
            img,
            to(x0, -0.005),
            to(x0 + (mid.0 - x0) * 0.3, mid.1 * 0.6),
            to(mid.0, mid.1),
            s * 0.013,
            s * 0.009,
            0.5 * sh,
            sh,
            base,
        );
        let top = if brown { tip } else { base };
        blade(img, to(mid.0, mid.1), to(ctrl.0, ctrl.1), to(tipx, tipy), s * 0.009, s * 0.003, sh, sh * 1.05, top);
    }
    darken_base(img, 0.35);
}

/// A cattail's flower stem: a thin green stalk with the brown velvet head
/// a third of the way down from the top and a bare spike above it. Shown
/// 0.14 as wide as tall, stiff in the wind.
fn cattail_head(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.14;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let stem = [0.08, 0.11, 0.04];
    let head = [0.17, 0.08, 0.03];
    let lean = r.range(-0.01, 0.01);
    blade(img, to(0.0, -0.005), to(lean * 0.5, 0.5), to(lean, 0.97), s * 0.09, s * 0.05, 0.75, 1.0, stem);
    // The head: a long brown sausage along the stalk, about seven times as tall
    // as it is thick once the card is squeezed to its 0.14 width, and about
    // three times the stalk. Drawn wide here because of that squeeze.
    let (cx, cy) = (lean * 0.8, 0.66);
    let (rx, ry) = (0.26, 0.135);
    paint(img, 0.0, 0.0, s, s, |x, y| {
        let p = to(cx, cy);
        let (dx, dy) = ((x - p[0]) / (s * rx), (y - p[1]) / (s * ry));
        let d = dx * dx + dy * dy;
        if d > 1.0 {
            return None;
        }
        // Rounder at the foot, blunter at the top, lit down one side, with a
        // fine velvet grain across it.
        let side = 0.74 + 0.34 * (dx + 1.0) * 0.5;
        let grain = 0.94 + 0.06 * ((dy * 46.0).sin() * (dx * 7.0).cos());
        let k = side * grain;
        Some((((1.0 - d) * 6.0).clamp(0.0, 1.0), [head[0] * k, head[1] * k, head[2] * k]))
    });
    // The bare spike above the head, thinner than the stalk below it.
    blade(img, to(cx, cy + ry * 0.95), to(cx, cy + ry * 1.5), to(lean, 0.99), s * 0.05, s * 0.025, 0.8, 1.0, stem);
    darken_base(img, 0.2);
}

/// A saw palmetto: a few fans of stiff grey-green blades on short stems, each
/// fan spread like an open hand. Shown 1.3 as wide as tall.
fn palmetto(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 1.3;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let green = [0.10, 0.14, 0.07];
    let stem = [0.10, 0.10, 0.05];
    for f in 0..5 {
        let back = if f < 2 { 0.75 } else { 1.0 };
        let sx = r.range(-0.12, 0.12);
        let (fx, fy) = (sx * 2.2, r.range(0.35, 0.62));
        // The petiole carries the fan, so it has to read at a distance: thin
        // enough and the fans look like they hover over the ground.
        blade(img, to(sx * 0.3, -0.005), to(sx, fy * 0.5), to(fx, fy), s * 0.015, s * 0.011, 0.6 * back, back, stem);
        // The fan: blades radiating over a half circle, tilted with its stem.
        let tilt = fx * 1.6;
        let n = 15;
        for k in 0..n {
            let a = std::f32::consts::PI * (0.08 + 0.84 * k as f32 / (n - 1) as f32) - tilt;
            let len = r.range(0.24, 0.32);
            let (ex, ey) = (fx + a.cos() * len * 0.9, fy + a.sin() * len);
            let (mx, my) = (fx + a.cos() * len * 0.5, fy + a.sin() * len * 0.55);
            let sh = r.range(0.85, 1.1) * back;
            blade(img, to(fx, fy), to(mx, my), to(ex, ey), s * 0.012, s * 0.004, 0.75 * sh, sh, green);
        }
    }
    darken_base(img, 0.25);
}

/// A clump of sugar cane: straight jointed stalks standing up together, long
/// ribbon leaves arching out from the upper half. Shown 0.45 as wide as tall.
fn cane(img: &mut Img, r: &mut Rng, s: f32) {
    const ASPECT: f32 = 0.45;
    let to = |x: f32, y: f32| [s * 0.5 + x * s / ASPECT, s * (1.0 - y)];
    let stalk = [0.10, 0.12, 0.05];
    let node = [0.07, 0.08, 0.03];
    let green = [0.08, 0.15, 0.04];
    let pale = [0.14, 0.19, 0.07];
    let n = 9;
    for i in 0..n {
        let x0 = r.range(-0.07, 0.07);
        let h = r.range(0.62, 0.8);
        let lean = r.range(-0.02, 0.02);
        let back = if i < n / 2 { 0.8 } else { 1.0 };
        blade(img, to(x0, -0.005), to(x0 + lean * 0.5, h * 0.5), to(x0 + lean, h), s * 0.018, s * 0.014, 0.7 * back, back, stalk);
        // Joints every few hand-spans.
        let mut y = 0.06;
        while y < h {
            dot(img, to(x0 + lean * y / h, y), s * 0.011, node);
            y += r.range(0.05, 0.08);
        }
        // Leaves from the upper half, arching out and over.
        for _ in 0..4 {
            let t = r.range(0.45, 1.0) * h;
            let side = if r.f() < 0.5 { -1.0 } else { 1.0 };
            let len = r.range(0.14, 0.26);
            let base = (x0 + lean * t / h, t);
            let tip = (base.0 + side * len, t + r.range(-0.12, 0.06));
            let ctrl = (base.0 + side * len * 0.45, t + len * 0.5);
            let col = if r.f() < 0.3 { pale } else { green };
            blade(img, to(base.0, base.1), to(ctrl.0, ctrl.1), to(tip.0, tip.1), s * 0.012, s * 0.003, 0.8 * back, back, col);
        }
        // The topmost leaves in a tuft.
        for _ in 0..3 {
            let side = r.range(-1.0, 1.0);
            let tip = (x0 + lean + side * 0.12, h + r.range(0.1, 0.2));
            blade(img, to(x0 + lean, h), to(x0 + lean + side * 0.05, h + 0.1), to(tip.0, tip.1), s * 0.011, s * 0.003, back, back, green);
        }
    }
    darken_base(img, 0.3);
}
