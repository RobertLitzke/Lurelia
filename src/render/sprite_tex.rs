//! Sprite textures, drawn on the CPU at startup: grass tufts, wildflowers, leaf
//! clusters and pine sprigs (layers in mesh/sprites.rs `layer`). Tinted kinds are
//! near grey, a brightness that the shader multiplies by the colour of the block
//! they grow from; flowers carry their own colours.
//!
//! Mips keep the share of opaque texels the same as the full-size image, so thin
//! blades don't fade to nothing in the distance under alpha-to-coverage.
//!
//! A PNG in library/sprites named after the layer (`NAMES`) replaces the drawn
//! one, so the art can be revised without touching code; library/sprites/README.md
//! is the spec. `cargo test --release export_sprites -- --ignored` writes the
//! current set there (it never overwrites a PNG that already exists).

use crate::mesh::sprites::layer;
use crate::world::noise::{hash32, unit};

/// Texture size of every layer. The grasses, flowers and leaves are drawn at
/// `SMALL` and scaled up; the big plants (fern fronds, fireweed) use it all.
pub const SIZE: usize = 256;
pub const MIPS: u32 = 9;
const SMALL: usize = 128;

pub(super) type Img = Vec<[f32; 4]>;

pub(super) struct Rng(pub u32);
impl Rng {
    pub(super) fn f(&mut self) -> f32 {
        self.0 = hash32(self.0.wrapping_add(0x9e37_79b9));
        unit(self.0)
    }
    pub(super) fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
}

/// Composite a shape over the image. `cov` gives (coverage, colour) per pixel
/// centre inside the box; coverage is already antialiased.
pub(super) fn paint(img: &mut Img, x0: f32, y0: f32, x1: f32, y1: f32, cov: impl Fn(f32, f32) -> Option<(f32, [f32; 3])>) {
    let n = side(img);
    let (xa, xb) = ((x0.floor() as i32).max(0), (x1.ceil() as i32).min(n as i32 - 1));
    let (ya, yb) = ((y0.floor() as i32).max(0), (y1.ceil() as i32).min(n as i32 - 1));
    for y in ya..=yb {
        for x in xa..=xb {
            if let Some((a, c)) = cov(x as f32 + 0.5, y as f32 + 0.5) {
                let a = a.clamp(0.0, 1.0);
                if a <= 0.0 {
                    continue;
                }
                let p = &mut img[y as usize * n + x as usize];
                for k in 0..3 {
                    p[k] = p[k] * (1.0 - a) + c[k] * a;
                }
                p[3] = p[3] + a * (1.0 - p[3]);
            }
        }
    }
}

/// A blade or stem: a quadratic curve from `a` through control `c` to `b`, `w0`
/// wide at the root tapering to `w1`, shaded from `s0` to `s1` (multiplied by `tint`).
pub(super) fn blade(img: &mut Img, a: [f32; 2], c: [f32; 2], b: [f32; 2], w0: f32, w1: f32, s0: f32, s1: f32, tint: [f32; 3]) {
    // Sample the curve; each pixel takes its nearest segment between samples.
    let n = 32;
    let pts: Vec<([f32; 2], f32, f32)> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let p = [u * u * a[0] + 2.0 * u * t * c[0] + t * t * b[0], u * u * a[1] + 2.0 * u * t * c[1] + t * t * b[1]];
            (p, w0 + (w1 - w0) * t, s0 + (s1 - s0) * t)
        })
        .collect();
    let pad = w0.max(w1) + 2.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (p, _, _) in &pts {
        x0 = x0.min(p[0] - pad);
        x1 = x1.max(p[0] + pad);
        y0 = y0.min(p[1] - pad);
        y1 = y1.max(p[1] + pad);
    }
    paint(img, x0, y0, x1, y1, |x, y| {
        let mut best = (f32::MAX, 0.0, 0.0);
        for pair in pts.windows(2) {
            let ((p, w, s), (q, w2, s2)) = (pair[0], pair[1]);
            let (ex, ey) = (q[0] - p[0], q[1] - p[1]);
            let f = (((x - p[0]) * ex + (y - p[1]) * ey) / (ex * ex + ey * ey).max(1e-6)).clamp(0.0, 1.0);
            let (w, s) = (w + (w2 - w) * f, s + (s2 - s) * f);
            let d = ((x - p[0] - ex * f).powi(2) + (y - p[1] - ey * f).powi(2)).sqrt() - w * 0.5;
            if d < best.0 {
                best = (d, w, s);
            }
        }
        // Thin strokes keep their weight as partial coverage.
        let cov = (0.5 - best.0).clamp(0.0, 1.0) * best.1.min(1.0);
        Some((cov, [tint[0] * best.2, tint[1] * best.2, tint[2] * best.2]))
    });
}

/// A pointed leaf from `p` along angle `ang`, `len` long and `w` wide.
pub(super) fn leaf(img: &mut Img, p: [f32; 2], ang: f32, len: f32, w: f32, shade: f32, tint: [f32; 3]) {
    let (dx, dy) = (ang.cos(), ang.sin());
    let r = len + 2.0;
    paint(img, p[0] - r, p[1] - r, p[0] + r, p[1] + r, |x, y| {
        let (rx, ry) = (x - p[0], y - p[1]);
        let s = rx * dx + ry * dy;
        let t = -rx * dy + ry * dx;
        if s < -1.0 || s > len + 1.0 {
            return None;
        }
        let half = w * 0.5 * (std::f32::consts::PI * (s / len).clamp(0.0, 1.0)).sin().max(0.0).powf(0.8);
        let cov = (half - t.abs() + 0.5).clamp(0.0, 1.0);
        // Darker midrib and edges, lighter toward the tip.
        let rib = if t.abs() < 0.7 { 0.85 } else { 1.0 };
        let edge = 0.85 + 0.15 * ((half - t.abs()) / 2.0).clamp(0.0, 1.0);
        let sh = shade * rib * edge * (0.9 + 0.15 * s / len);
        Some((cov, [tint[0] * sh, tint[1] * sh, tint[2] * sh]))
    });
}

pub(super) fn dot(img: &mut Img, p: [f32; 2], r: f32, col: [f32; 3]) {
    paint(img, p[0] - r - 1.0, p[1] - r - 1.0, p[0] + r + 1.0, p[1] + r + 1.0, |x, y| {
        let d = ((x - p[0]).powi(2) + (y - p[1]).powi(2)).sqrt();
        Some(((r - d + 0.5).clamp(0.0, 1.0), col))
    });
}

const GREY: [f32; 3] = [1.0, 1.0, 1.0];
/// Side of the small layers' canvas.
const S: f32 = SMALL as f32;

fn side(img: &Img) -> usize {
    (img.len() as f32).sqrt().round() as usize
}

/// Blades rising across most of the card's width, crowded toward the middle,
/// the outer ones shorter and leaning outward, so a tuft reads as a mound and
/// leaves little bare ground between neighbours. `n` blades, heights in `hs`
/// (share of the image), `lean` how far tips wander sideways. Blades drawn
/// first sit behind and are a little darker.
fn tuft(img: &mut Img, r: &mut Rng, n: usize, hs: (f32, f32), lean: f32, width: f32) {
    const SPREAD: f32 = 0.36;
    for i in 0..n {
        let off = (r.f() + r.f() - 1.0) * SPREAD;
        let edge = off.abs() / SPREAD;
        let bx = S * (0.5 + off);
        let h = r.range(hs.0, hs.1) * (1.0 - 0.35 * edge) * S;
        let tx = (0.5 + off * 1.5 + r.range(-lean, lean)).clamp(0.03, 0.97);
        let tip = [tx * S, S - h];
        let ctrl = [bx + (tip[0] - bx) * 0.2, S - h * 0.55];
        let back = if i < n / 2 { 0.85 } else { 1.0 };
        let s = r.range(0.85, 1.0) * back;
        let warm = [1.0, 1.0, r.range(0.82, 0.95)];
        blade(img, [bx, S + 1.0], ctrl, tip, width * r.range(0.8, 1.2), 0.6, 0.42 * s, s, warm);
    }
}

fn draw_layer(l: u32) -> Img {
    if !layer::small(l) {
        return super::sprite_plants::draw(l, SIZE);
    }
    let mut img = vec![[0.0; 4]; SMALL * SMALL];
    let mut r = Rng(l.wrapping_mul(7919) + 13);
    match l {
        layer::TUFT_SHORT => tuft(&mut img, &mut r, 30, (0.35, 0.75), 0.22, 4.0),
        layer::TUFT_TALL => tuft(&mut img, &mut r, 34, (0.55, 0.98), 0.18, 3.2),
        layer::TUFT_SEED => {
            tuft(&mut img, &mut r, 24, (0.4, 0.8), 0.2, 3.2);
            for _ in 0..4 {
                let bx = S * 0.5 + r.range(-0.08, 0.08) * S;
                let h = r.range(0.8, 0.97) * S;
                let tip = [bx + r.range(-0.18, 0.18) * S, S - h];
                let ctrl = [bx + (tip[0] - bx) * 0.3, S - h * 0.5];
                blade(&mut img, [bx, S + 1.0], ctrl, tip, 1.8, 1.2, 0.5, 0.95, [1.0, 0.97, 0.8]);
                // Seed head: a slim oval along the top of the stalk.
                let ang = (tip[1] - ctrl[1]).atan2(tip[0] - ctrl[0]);
                leaf(&mut img, tip, ang + std::f32::consts::PI, 0.16 * S, 5.0, 1.0, [1.0, 0.95, 0.72]);
            }
        }
        layer::FLOWERS_WHITE | layer::FLOWERS_BLUE => {
            let stem = [0.10, 0.20, 0.05];
            // A little grass at the foot so the flowers sit in the meadow.
            for _ in 0..8 {
                let bx = S * 0.5 + r.range(-0.12, 0.12) * S;
                let h = r.range(0.25, 0.5) * S;
                let tip = [bx + r.range(-0.3, 0.3) * S, S - h];
                blade(&mut img, [bx, S + 1.0], [bx, S - h * 0.5], tip, 3.5, 0.6, 0.5, 1.0, [0.16, 0.26, 0.07]);
            }
            for _ in 0..if l == layer::FLOWERS_WHITE { 6 } else { 4 } {
                let bx = S * 0.5 + r.range(-0.15, 0.15) * S;
                let h = r.range(0.5, 0.95) * S;
                let tip = [bx + r.range(-0.2, 0.2) * S, S - h];
                blade(&mut img, [bx, S + 1.0], [bx, S - h * 0.5], tip, 2.2, 1.6, 0.6, 1.0, stem);
                if l == layer::FLOWERS_WHITE {
                    // A daisy seen from the side and a little above: petals round a yellow eye.
                    let pr = r.range(5.0, 8.0);
                    for k in 0..7 {
                        let a = k as f32 / 7.0 * std::f32::consts::TAU + r.range(0.0, 0.3);
                        let q = [tip[0] + a.cos() * pr * 0.2, tip[1] + a.sin() * pr * 0.12];
                        leaf(&mut img, q, a.sin().atan2(a.cos() * 1.0), pr, pr * 0.55, 1.0, [0.88, 0.86, 0.80]);
                    }
                    dot(&mut img, tip, pr * 0.35, [0.85, 0.6, 0.08]);
                } else {
                    // A lupine spike: florets shrinking toward the top.
                    let n = 9;
                    for k in 0..n {
                        let t = k as f32 / n as f32;
                        let y = tip[1] + t * 0.24 * S;
                        let rad = 1.4 + 2.2 * t;
                        let col = [0.20 + 0.15 * t, 0.18 + 0.1 * t, 0.62 + 0.15 * t];
                        dot(&mut img, [tip[0] - rad * 0.6, y], rad, col);
                        dot(&mut img, [tip[0] + rad * 0.6, y + 2.0], rad, [col[0] * 0.8, col[1] * 0.8, col[2] * 0.85]);
                    }
                }
            }
        }
        layer::FLOWERS_YELLOW | layer::FLOWERS_RED => {
            // Same build as the white and blue: grass at the foot, a few tall stems.
            let stem = [0.10, 0.20, 0.05];
            for _ in 0..8 {
                let bx = S * 0.5 + r.range(-0.12, 0.12) * S;
                let h = r.range(0.25, 0.5) * S;
                let tip = [bx + r.range(-0.3, 0.3) * S, S - h];
                blade(&mut img, [bx, S + 1.0], [bx, S - h * 0.5], tip, 3.5, 0.6, 0.5, 1.0, [0.16, 0.26, 0.07]);
            }
            let yellow = l == layer::FLOWERS_YELLOW;
            for _ in 0..5 {
                let bx = S * 0.5 + r.range(-0.17, 0.17) * S;
                let h = if yellow { r.range(0.5, 0.92) } else { r.range(0.45, 0.8) } * S;
                let tip = [bx + r.range(-0.2, 0.2) * S, S - h];
                blade(&mut img, [bx, S + 1.0], [bx, S - h * 0.5], tip, 2.2, 1.6, 0.6, 1.0, stem);
                if yellow {
                    // A balsamroot-like sunflower head: broad yellow rays round a
                    // brown-gold eye, seen a little from above.
                    let pr = r.range(6.0, 9.0);
                    for k in 0..9 {
                        let a = k as f32 / 9.0 * std::f32::consts::TAU + r.range(0.0, 0.25);
                        let q = [tip[0] + a.cos() * pr * 0.18, tip[1] + a.sin() * pr * 0.1];
                        let tone = r.range(0.85, 1.0);
                        leaf(&mut img, q, a.sin().atan2(a.cos() * 1.0), pr, pr * 0.6, 1.0, [0.95 * tone, 0.72 * tone, 0.10 * tone]);
                    }
                    dot(&mut img, tip, pr * 0.38, [0.45, 0.28, 0.06]);
                } else {
                    // Paintbrush: a short, blunt spike of scarlet bracts over a
                    // few green leaves.
                    let n = 8;
                    for k in 0..n {
                        let t = k as f32 / n as f32;
                        let y = tip[1] + t * 0.16 * S;
                        let rad = 2.4 + 1.6 * t.min(0.6);
                        let tone = r.range(0.85, 1.05);
                        let col = [0.86 * tone, (0.16 + 0.10 * (1.0 - t)) * tone, 0.07 * tone];
                        dot(&mut img, [tip[0] - rad * 0.55, y], rad, col);
                        dot(&mut img, [tip[0] + rad * 0.55, y + 1.5], rad, [col[0] * 0.8, col[1] * 0.8, col[2]]);
                    }
                    for side in [-1.0f32, 1.0] {
                        let p = [tip[0], tip[1] + 0.24 * S];
                        leaf(&mut img, p, -std::f32::consts::FRAC_PI_2 + side * 0.7, 0.1 * S, 3.0, 0.9, [0.14, 0.24, 0.07]);
                    }
                }
            }
        }
        layer::VINE_FLOWERS => {
            // A length of vine crossing the card: leaves alternating along the
            // strand on short stalks, and two or three sprays of small pale
            // yellow flowers hanging off nodes, so nothing floats loose.
            let wood = [0.24, 0.22, 0.13];
            let leaf_col = [0.20, 0.34, 0.12];
            let a0 = r.range(0.0, std::f32::consts::TAU);
            let (p0, p1) = (
                [S * 0.5 - a0.cos() * S * 0.46, S * 0.5 - a0.sin() * S * 0.46],
                [S * 0.5 + a0.cos() * S * 0.46, S * 0.5 + a0.sin() * S * 0.46],
            );
            // The strand sags between its ends.
            let perp = [-a0.sin(), a0.cos()];
            let sag = r.range(0.1, 0.22) * S * if r.f() < 0.5 { 1.0 } else { -1.0 };
            let cm = [S * 0.5 + perp[0] * sag, S * 0.5 + perp[1] * sag];
            let at = |t: f32| {
                let u = 1.0 - t;
                [u * u * p0[0] + 2.0 * u * t * cm[0] + t * t * p1[0], u * u * p0[1] + 2.0 * u * t * cm[1] + t * t * p1[1]]
            };
            blade(&mut img, p0, cm, p1, 2.6, 2.0, 0.6, 0.85, wood);
            let n = 9;
            for k in 0..n {
                let t = (k as f32 + r.range(0.2, 0.8)) / n as f32;
                let q = at(t);
                let d = at((t + 0.02).min(1.0));
                let ang = (d[1] - q[1]).atan2(d[0] - q[0]);
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let out = ang + side * r.range(0.8, 1.25);
                // A short stalk, then the leaf on the end of it.
                let stalk = r.range(4.0, 7.0);
                let e = [q[0] + out.cos() * stalk, q[1] + out.sin() * stalk];
                blade(&mut img, q, [(q[0] + e[0]) * 0.5, (q[1] + e[1]) * 0.5], e, 1.4, 1.0, 0.5, 0.7, wood);
                let len = r.range(13.0, 19.0);
                leaf(&mut img, e, out + r.range(-0.25, 0.25), len, len * r.range(0.6, 0.75), r.range(0.7, 0.95), leaf_col);
            }
            for _ in 0..r.range(2.0, 3.99) as usize {
                let t = r.range(0.2, 0.8);
                let q = at(t);
                let d = at((t + 0.02).min(1.0));
                let ang = (d[1] - q[1]).atan2(d[0] - q[0]);
                let hang = ang + if r.f() < 0.5 { 1.4 } else { -1.4 };
                let reach = r.range(10.0, 18.0);
                let c = [q[0] + hang.cos() * reach, q[1] + hang.sin() * reach];
                blade(&mut img, q, [(q[0] + c[0]) * 0.5, (q[1] + c[1]) * 0.5], c, 1.4, 0.9, 0.5, 0.7, wood);
                // Three or four little flowers on the end of the spray.
                for _ in 0..r.range(3.0, 4.99) as usize {
                    let fc = [c[0] + r.range(-6.0, 6.0), c[1] + r.range(-6.0, 6.0)];
                    let pr = r.range(4.0, 6.0);
                    let tone = r.range(0.9, 1.0);
                    for k in 0..5 {
                        let pa = k as f32 / 5.0 * std::f32::consts::TAU + r.range(0.0, 0.5);
                        let q2 = [fc[0] + pa.cos() * pr * 0.5, fc[1] + pa.sin() * pr * 0.5];
                        dot(&mut img, q2, pr * 0.5, [0.96 * tone, 0.90 * tone, 0.58 * tone]);
                    }
                    dot(&mut img, fc, pr * 0.3, [0.88, 0.66, 0.20]);
                }
            }
        }
        layer::GLOW_THREADS => {
            // Fine threads hanging from the top of the card, each strung with
            // drops of blue light that grow toward the lower end.
            for _ in 0..r.range(7.0, 10.99) as usize {
                let x = S * r.range(0.15, 0.85);
                let len = S * r.range(0.35, 0.9);
                let top = S * 0.02;
                let sway = r.range(-0.04, 0.04) * S;
                blade(&mut img, [x, top], [x + sway * 0.5, top + len * 0.5], [x + sway, top + len], 1.8, 1.1, 0.6, 0.8, [0.12, 0.30, 0.55]);
                let drops = r.range(4.0, 8.99) as usize;
                for k in 0..drops {
                    let t = (k as f32 + r.range(0.2, 0.8)) / drops as f32;
                    let p = [x + sway * t * t, top + len * t];
                    dot(&mut img, p, 1.3 + 1.6 * t, [0.12, 0.42, 1.0]);
                }
            }
        }
        layer::WISTERIA => {
            // Two or three racemes hanging from the top of the card (this layer
            // keeps the camera's up, mesh/sprites.rs `upright`), each a tapering
            // cone of pea flowers that thins to single florets at the tip.
            //
            // Painted pale and tinted by the block it grows on, rather than in
            // wisteria's own violet: the vine comes in coats now (violet,
            // lilac, a rare white), and one fixed raceme would have left every
            // coat flowering the same colour. The ramp from shoulder to tip is
            // kept as brightness, so each coat still pales toward the tip.
            // The leaves come from the LEAVES_B card beside it.
            let stem = [0.30, 0.30, 0.26];
            let n = r.range(2.0, 3.99) as usize;
            let mut hang = Vec::new();
            for j in 0..n {
                let x0 = S * (0.22 + 0.56 * (j as f32 + r.range(0.2, 0.8)) / n as f32);
                hang.push(x0);
            }
            for x0 in hang {
                let len = S * r.range(0.58, 0.84);
                let top = S * 0.1;
                let lean = r.range(-0.07, 0.07) * S;
                let warm = r.range(0.0, 1.0);
                let m = 15;
                blade(&mut img, [x0, top - 6.0], [x0, top + len * 0.3], [x0 + lean, top + len], 2.0, 1.0, 0.5, 0.6, stem);
                for k in 0..m {
                    let t = k as f32 / (m - 1) as f32;
                    let y = top + t * len;
                    let x = x0 + lean * t * t;
                    // Full colour at the shoulder -> paling to near white at
                    // the tip, with a touch of warmth raceme by raceme. The
                    // block's own colour multiplies this.
                    let v = 0.62 + 0.38 * t;
                    let col = [v * (1.0 + 0.05 * warm), v, v * (1.0 + 0.06 * (1.0 - warm))];
                    // Florets thin out and shrink toward the tip, and the last
                    // few hang singly, so the raceme ends in a point.
                    let rad = 3.6 * (1.0 - 0.75 * t) + 0.7;
                    let spread = 6.0 * (1.0 - 0.8 * t);
                    let each = if t < 0.55 {
                        2
                    } else if t < 0.85 {
                        if r.f() < 0.6 {
                            2
                        } else {
                            1
                        }
                    } else {
                        1
                    };
                    for i in 0..each {
                        let side = if each == 1 { r.range(-0.6, 0.6) } else { (i as f32 * 2.0 - 1.0) * r.range(0.6, 1.1) };
                        let shade = r.range(0.82, 1.0);
                        let c = [x + side * spread, y + r.range(-1.2, 1.2)];
                        let col = [col[0] * shade, col[1] * shade, col[2] * shade];
                        // A pea flower: the standard petal above, the keel below.
                        dot(&mut img, [c[0], c[1] - rad * 0.25], rad, col);
                        dot(&mut img, [c[0], c[1] + rad * 0.5], rad * 0.65, [col[0] * 0.82, col[1] * 0.78, col[2] * 0.88]);
                    }
                }
            }
        }
        layer::LILY_FLOWER => super::sprite_plants::lily_flower(&mut img, &mut r, S),
        layer::ROSES => {
            // A rose sprig: a woody stem up from the bottom edge, forking twice,
            // leaves in opposite pairs along it and an open bloom at the end of
            // each fork, with a leaf or two tucked behind the bloom so the
            // flowers and leaves read as one plant.
            let cane = [0.22, 0.20, 0.12];
            let leaf_col = [0.16, 0.30, 0.10];
            let root = [S * 0.5, S * 0.99];
            let bow = r.range(-0.12, 0.12) * S;
            let tipp = [S * 0.5 + bow, S * 0.22];
            let ctrl = [S * 0.5 + bow * 0.4, S * 0.6];
            let at = |t: f32| {
                let u = 1.0 - t;
                [u * u * root[0] + 2.0 * u * t * ctrl[0] + t * t * tipp[0], u * u * root[1] + 2.0 * u * t * ctrl[1] + t * t * tipp[1]]
            };
            blade(&mut img, root, ctrl, tipp, 2.0, 1.0, 0.55, 0.8, cane);
            let mut heads = vec![(tipp, (tipp[1] - ctrl[1]).atan2(tipp[0] - ctrl[0]))];
            for (t0, side) in [(0.22f32, -1.0f32), (0.42, 1.0), (0.6, -1.0), (0.76, 1.0)] {
                let p0 = at(t0);
                let len = S * r.range(0.17, 0.27);
                let a = -std::f32::consts::FRAC_PI_2 + side * r.range(0.6, 1.05);
                let p1 = [p0[0] + a.cos() * len, p0[1] + a.sin() * len];
                blade(&mut img, p0, [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5], p1, 1.5, 0.9, 0.5, 0.75, cane);
                // Leaflets in opposite pairs along the fork.
                for k in 0..4 {
                    let t = (k as f32 + r.range(0.3, 0.9)) / 4.0;
                    let q = [p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t];
                    let len = r.range(9.0, 14.0);
                    for ls in [-1.0f32, 1.0] {
                        leaf(&mut img, q, a + ls * r.range(0.6, 0.95), len, len * 0.55, r.range(0.7, 0.95), leaf_col);
                    }
                }
                heads.push((p1, a));
                if r.f() < 0.6 {
                    let t = r.range(0.45, 0.7);
                    heads.push(([p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t], a + r.range(-0.9, 0.9)));
                }
            }
            for (c, a) in heads {
                // Two leaves behind the bloom, then the bloom over them.
                for ls in [-1.0f32, 1.0] {
                    let len = r.range(12.0, 17.0);
                    leaf(&mut img, c, a + ls * r.range(0.5, 1.1), len, len * 0.55, r.range(0.6, 0.85), leaf_col);
                }
                let pr = r.range(7.0, 9.5);
                let tone = r.range(0.85, 1.0);
                // Outer petals, then an inner whorl, then the eye.
                for k in 0..7 {
                    let pa = k as f32 / 7.0 * std::f32::consts::TAU + r.range(0.0, 0.5);
                    let q = [c[0] + pa.cos() * pr * 0.48, c[1] + pa.sin() * pr * 0.48];
                    dot(&mut img, q, pr * 0.46, [0.90 * tone, 0.50 * tone, 0.62 * tone]);
                }
                for k in 0..4 {
                    let pa = k as f32 / 4.0 * std::f32::consts::TAU + r.range(0.0, 0.8);
                    let q = [c[0] + pa.cos() * pr * 0.24, c[1] + pa.sin() * pr * 0.24];
                    dot(&mut img, q, pr * 0.3, [0.82 * tone, 0.36 * tone, 0.50 * tone]);
                }
                dot(&mut img, c, pr * 0.16, [0.95, 0.80, 0.40]);
            }
        }
        layer::LEAVES_A => {
            // The filler: four clusters of small leaves round the middle, each on
            // its own twig, their centres solid and a little sky left between
            // them. Not a rosette of big leaves from one point, which stamps the
            // same star over the whole crown.
            //
            // Value is flat within a cluster and steps between them (back ones
            // darker), so each leaf keeps a crisp silhouette at distance instead
            // of smearing into a gradient.
            let c = [S * 0.5, S * 0.5];
            let groups = 7;
            for g in 0..groups {
                let a = (g as f32 + r.range(0.15, 0.85)) / groups as f32 * std::f32::consts::TAU;
                let d = S * r.range(0.10, 0.21);
                let anchor = [c[0] + a.cos() * d, c[1] + a.sin() * d];
                let mid = [(c[0] + anchor[0]) * 0.5, (c[1] + anchor[1]) * 0.5];
                blade(&mut img, c, mid, anchor, 2.6, 1.5, 0.38, 0.48, GREY);
                // Back clusters first and darker; the front ones overlap them.
                let step = [0.56, 0.63, 0.70, 0.77, 0.84, 0.91, 0.98][g];
                for _ in 0..r.range(9.0, 12.99) as usize {
                    let p = [anchor[0] + r.range(-0.06, 0.06) * S, anchor[1] + r.range(-0.06, 0.06) * S];
                    let len = r.range(18.0, 28.0);
                    leaf(&mut img, p, a + r.range(-0.7, 0.7), len, len * r.range(0.55, 0.7), step * r.range(0.96, 1.04), GREY);
                }
            }
            // A few over the middle so the crown behind never shows through it.
            for _ in 0..10 {
                let a = r.range(0.0, std::f32::consts::TAU);
                let len = r.range(16.0, 23.0);
                leaf(&mut img, [c[0] + r.range(-8.0, 8.0), c[1] + r.range(-8.0, 8.0)], a, len, len * 0.65, r.range(0.7, 0.86), GREY);
            }
        }
        layer::LEAVES_B => {
            // The rim: a sparser spray on a forking twig that leaves the card at
            // the bottom edge, so where it sticks out past the crown it reads as
            // a leafy branch tip rather than a second clump.
            let root = [S * 0.5, S * 0.96];
            let bow = r.range(-0.18, 0.18) * S;
            let tip = [S * 0.5 + bow, S * 0.2];
            let ctrl = [S * 0.5 + bow * 0.3, S * 0.52];
            let at = |t: f32| {
                let u = 1.0 - t;
                [u * u * root[0] + 2.0 * u * t * ctrl[0] + t * t * tip[0], u * u * root[1] + 2.0 * u * t * ctrl[1] + t * t * tip[1]]
            };
            blade(&mut img, root, ctrl, tip, 3.0, 1.2, 0.36, 0.46, GREY);
            // Leaves along the twig and its two forks, all pointing forward.
            let spray = |img: &mut Img, r: &mut Rng, p0: [f32; 2], p1: [f32; 2], n: usize, shade: f32| {
                let ang = (p1[1] - p0[1]).atan2(p1[0] - p0[0]);
                for k in 0..n {
                    // Uneven spacing, and now and then a gap where none grows.
                    let t = (k as f32 + r.range(0.1, 0.9)) / n as f32;
                    if r.f() < 0.08 {
                        continue;
                    }
                    let q = [p0[0] + (p1[0] - p0[0]) * t, p0[1] + (p1[1] - p0[1]) * t];
                    let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                    let len = r.range(15.0, 23.0) * (1.0 - 0.22 * t);
                    leaf(img, q, ang + side * r.range(0.45, 0.85), len, len * r.range(0.56, 0.7), shade * r.range(0.96, 1.04), GREY);
                }
            };
            for (t0, side) in [(0.22f32, -1.0f32), (0.48, 1.0), (0.68, -1.0)] {
                let p0 = at(t0);
                let len = S * r.range(0.2, 0.28);
                let a = -std::f32::consts::FRAC_PI_2 + side * r.range(0.6, 0.95);
                let p1 = [p0[0] + a.cos() * len, p0[1] + a.sin() * len];
                blade(&mut img, p0, [(p0[0] + p1[0]) * 0.5, (p0[1] + p1[1]) * 0.5], p1, 2.2, 1.0, 0.36, 0.44, GREY);
                spray(&mut img, &mut r, p0, p1, 7, if side < 0.0 { 0.66 } else { 0.8 });
            }
            spray(&mut img, &mut r, at(0.1), tip, 13, 0.94);
        }
        layer::NEEDLES => {
            // A fir branch seen from above: a twig from the bottom centre (where
            // it leaves the crown) to the top edge, side shoots angled toward
            // the tip, longest near the trunk, all fringed with short needles.
            // The twig bows and the shoots curve back toward the root, so the
            // spray reads as grown rather than combed.
            // Kept dark, near the crown's own colour (77% grey is the block colour).
            let root = [S * 0.5, S];
            let bow = r.range(0.06, 0.13) * S * if r.f() < 0.5 { 1.0 } else { -1.0 };
            let tip = [S * 0.5 + bow * 0.4, S * 0.04];
            let ctrl = [S * 0.5 + bow * 2.2, S * 0.52];
            let at = |t: f32| {
                let u = 1.0 - t;
                [u * u * root[0] + 2.0 * u * t * ctrl[0] + t * t * tip[0], u * u * root[1] + 2.0 * u * t * ctrl[1] + t * t * tip[1]]
            };
            // Needles down a shoot, curving toward its tip and shortening.
            let needles = |img: &mut Img, r: &mut Rng, p0: [f32; 2], p1: [f32; 2], c: [f32; 2], len: f32| {
                let span = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
                let steps = (span / 2.2) as usize;
                for k in 0..steps {
                    let t = (k as f32 + r.range(0.2, 0.8)) / steps as f32;
                    let u = 1.0 - t;
                    let q = [u * u * p0[0] + 2.0 * u * t * c[0] + t * t * p1[0], u * u * p0[1] + 2.0 * u * t * c[1] + t * t * p1[1]];
                    // Tangent here, so needles sweep with the curve of the shoot.
                    let d = [2.0 * u * (c[0] - p0[0]) + 2.0 * t * (p1[0] - c[0]), 2.0 * u * (c[1] - p0[1]) + 2.0 * t * (p1[1] - c[1])];
                    let ang = d[1].atan2(d[0]);
                    for side in [-1.0f32, 1.0] {
                        let na = ang + side * r.range(0.65, 1.05);
                        let nl = len * (1.0 - 0.45 * t) * r.range(0.75, 1.15);
                        let e = [q[0] + na.cos() * nl, q[1] + na.sin() * nl];
                        // Each needle droops a little away from the shoot.
                        let m = [q[0] + na.cos() * nl * 0.5 - d[0] * 0.04, q[1] + na.sin() * nl * 0.5 - d[1] * 0.04];
                        let sh = r.range(0.62, 0.86);
                        blade(img, q, m, e, 1.6, 0.8, sh * 0.85, sh, GREY);
                    }
                }
            };
            let shoots = 13;
            for k in 1..shoots {
                // Uneven spacing up the twig, so the pairs do not read as a comb.
                let t = (0.06 + (k as f32 + r.range(-0.3, 0.3)) / shoots as f32 * 0.86).clamp(0.05, 0.95);
                let side = if k % 2 == 0 { 1.0 } else { -1.0 };
                let p0 = at(t);
                let len = S * 0.34 * (1.0 - t).powf(0.6) * r.range(0.75, 1.15);
                let ang = -std::f32::consts::FRAC_PI_2 + side * r.range(0.7, 1.0);
                let p1 = [p0[0] + ang.cos() * len, p0[1] + ang.sin() * len];
                // The shoot bends back toward the root over its length.
                let c = [p0[0] + ang.cos() * len * 0.55, p0[1] + ang.sin() * len * 0.55 + len * r.range(0.1, 0.2)];
                blade(&mut img, p0, c, p1, 1.8, 1.0, 0.42, 0.5, GREY);
                needles(&mut img, &mut r, p0, p1, c, 7.0);
            }
            blade(&mut img, root, ctrl, tip, 3.0, 1.2, 0.38, 0.5, GREY);
            needles(&mut img, &mut r, at(0.05), tip, ctrl, 8.0);
        }
        _ => {}
    }
    img
}

/// File names (without .png) of the layers, in layer order.
pub const NAMES: [&str; layer::COUNT as usize] = [
    "tuft_short",
    "tuft_tall",
    "tuft_seed",
    "flowers_white",
    "flowers_blue",
    "leaves_a",
    "leaves_b",
    "needles",
    "frond",
    "fireweed",
    "heather",
    "glassbloom",
    "sage",
    "tuft_long",
    "rabbitbrush",
    "tuft_patch",
    "flowers_yellow",
    "flowers_red",
    "huckleberry",
    "thimbleberry",
    "agave_stalk",
    "agave_stalk_dry",
    "yucca_stalk",
    "roses",
    "cattail",
    "cattail_head",
    "sawgrass",
    "palmetto",
    "cane",
    "vine_flowers",
    "wisteria",
    "glow_threads",
    "lily_pad",
    "lily_flower",
];

fn sprite_dir() -> std::path::PathBuf {
    crate::library::data_root().join("library").join("sprites")
}

/// A layer from library/sprites, as straight-alpha linear colour at its own
/// size, if the file is there, readable and square at 128, 256 or 512.
fn load(name: &str) -> Option<Img> {
    let path = sprite_dir().join(format!("{name}.png"));
    let file = std::fs::File::open(&path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = match dec.read_info() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("sprites: {}: {e}", path.display());
            return None;
        }
    };
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Grayscale => 1,
        _ => return None,
    };
    if w != h || ![128, 256, 512].contains(&w) {
        eprintln!("sprites: {} is {w}x{h}; it must be square, 128, 256 or 512", path.display());
        return None;
    }
    let texel = |x: usize, y: usize| -> [f32; 4] {
        let i = (y * w + x) * ch;
        let lin = |v: u8| (v as f32 / 255.0).powf(2.2);
        match ch {
            4 => [lin(buf[i]), lin(buf[i + 1]), lin(buf[i + 2]), buf[i + 3] as f32 / 255.0],
            3 => [lin(buf[i]), lin(buf[i + 1]), lin(buf[i + 2]), 1.0],
            2 => [lin(buf[i]), lin(buf[i]), lin(buf[i]), buf[i + 1] as f32 / 255.0],
            _ => [lin(buf[i]), lin(buf[i]), lin(buf[i]), 1.0],
        }
    };
    let mut img = vec![[0.0; 4]; w * w];
    for y in 0..w {
        for x in 0..w {
            img[y * w + x] = texel(x, y);
        }
    }
    Some(img)
}

/// Layer `l` as straight-alpha linear colour at SIZE: the PNG if there is one,
/// else drawn.
fn layer_image(l: u32) -> Img {
    if let Some(img) = load(NAMES[l as usize]) {
        return resize(&img);
    }
    // Painting over transparent black leaves colour premultiplied: divide it out.
    let mut img = draw_layer(l);
    for p in img.iter_mut() {
        for k in 0..3 {
            p[k] = if p[3] > 1e-3 { p[k] / p[3] } else { 0.0 };
        }
    }
    resize(&img)
}

/// Bring a square straight-alpha image to SIZE: box-filtered down by whole
/// factors, or bilinear up. Colour is weighted by alpha so edges stay clean.
fn resize(img: &Img) -> Img {
    let n = side(img);
    if n == SIZE {
        return img.clone();
    }
    let mut out = vec![[0.0; 4]; SIZE * SIZE];
    let get = |x: isize, y: isize| img[y.clamp(0, n as isize - 1) as usize * n + x.clamp(0, n as isize - 1) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let mut taps: Vec<([f32; 4], f32)> = Vec::new();
            if n > SIZE {
                let f = n / SIZE;
                for dy in 0..f {
                    for dx in 0..f {
                        taps.push((get((x * f + dx) as isize, (y * f + dy) as isize), 1.0));
                    }
                }
            } else {
                let fx = (x as f32 + 0.5) * n as f32 / SIZE as f32 - 0.5;
                let fy = (y as f32 + 0.5) * n as f32 / SIZE as f32 - 0.5;
                let (x0, y0) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - x0, fy - y0);
                let (x0, y0) = (x0 as isize, y0 as isize);
                taps.push((get(x0, y0), (1.0 - tx) * (1.0 - ty)));
                taps.push((get(x0 + 1, y0), tx * (1.0 - ty)));
                taps.push((get(x0, y0 + 1), (1.0 - tx) * ty));
                taps.push((get(x0 + 1, y0 + 1), tx * ty));
            }
            let (mut acc, mut wa, mut wt) = ([0f32; 4], 0f32, 0f32);
            for (t, w) in &taps {
                for k in 0..3 {
                    acc[k] += t[k] * t[3] * w;
                }
                acc[3] += t[3] * w;
                wa += t[3] * w;
                wt += w;
            }
            for k in 0..3 {
                acc[k] = if wa > 0.0 { acc[k] / wa } else { 0.0 };
            }
            acc[3] /= wt.max(1e-6);
            out[y * SIZE + x] = acc;
        }
    }
    out
}

fn to_srgb(c: f32) -> u8 {
    (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0).round() as u8
}

fn coverage(img: &Img, scale: f32) -> f32 {
    img.iter().filter(|p| p[3] * scale >= 0.5).count() as f32 / img.len() as f32
}

/// RGBA8 sRGB data for every mip of every layer: `out[mip][layer]`.
pub fn build() -> Vec<Vec<Vec<u8>>> {
    let mut mips = vec![Vec::new(); MIPS as usize];
    for l in 0..layer::COUNT {
        let mut img = layer_image(l);
        // Transparent texels take the mean colour, so filtering never pulls in black.
        let (mut sum, mut wsum) = ([0f32; 3], 0f32);
        for p in &img {
            for k in 0..3 {
                sum[k] += p[k] * p[3];
            }
            wsum += p[3];
        }
        let mean = sum.map(|s| s / wsum.max(1e-6));
        for p in img.iter_mut() {
            if p[3] <= 1e-3 {
                *p = [mean[0], mean[1], mean[2], 0.0];
            }
        }
        let target = coverage(&img, 1.0);
        let mut size = SIZE;
        for m in 0..MIPS as usize {
            if m > 0 {
                let ns = size / 2;
                let mut next = vec![[0f32; 4]; ns * ns];
                for y in 0..ns {
                    for x in 0..ns {
                        let mut acc = [0f32; 4];
                        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                            let s = img[(y * 2 + dy) * size + x * 2 + dx];
                            let w = s[3] + 1e-4;
                            for k in 0..3 {
                                acc[k] += s[k] * w;
                            }
                            acc[3] += s[3] * 0.25;
                        }
                        let w: f32 = [(0, 0), (1, 0), (0, 1), (1, 1)].iter().map(|&(dx, dy)| img[(y * 2 + dy) * size + x * 2 + dx][3] + 1e-4).sum();
                        next[y * ns + x] = [acc[0] / w, acc[1] / w, acc[2] / w, acc[3]];
                    }
                }
                img = next;
                size = ns;
            }
            // Scale alpha so this mip covers as much as the full-size image.
            let (mut lo, mut hi) = (1.0f32, 8.0f32);
            if m > 0 && size >= 4 {
                for _ in 0..16 {
                    let mid = (lo + hi) * 0.5;
                    if coverage(&img, mid) < target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
            }
            let scale = if m > 0 && size >= 4 { hi } else { 1.0 };
            let mut bytes = vec![0u8; size * size * 4];
            for (i, p) in img.iter().enumerate() {
                for k in 0..3 {
                    bytes[i * 4 + k] = to_srgb(p[k]);
                }
                bytes[i * 4 + 3] = ((p[3] * scale).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
            mips[m].push(bytes);
        }
    }
    mips
}

/// The colour each tinted layer is multiplied by in the previews: about what
/// the calm style's grass, leaves and pine give in the game (the shader also
/// scales tinted sprites by 1.3). Flowers are untinted.
#[cfg(test)]
const PREVIEW_TINT: [[f32; 3]; layer::COUNT as usize] = [
    [0.30, 0.45, 0.10],
    [0.30, 0.45, 0.10],
    [0.30, 0.45, 0.10],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.12, 0.30, 0.07],
    [0.12, 0.30, 0.07],
    [0.05, 0.18, 0.09],
    [0.25, 0.42, 0.16],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.45, 0.48, 0.38],
    [0.30, 0.45, 0.10],
    [1.0, 1.0, 1.0],
    [0.30, 0.45, 0.10],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.26, 0.42, 0.16],
    [1.0, 1.0, 1.0],
];

#[cfg(test)]
fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> std::io::Result<()> {
    crate::util::save_png(path, w, h, rgba)
}

/// Write each layer as an editable PNG (straight alpha, sRGB) into `dir`,
/// skipping files that exist unless `overwrite`, plus sheet.png (the layers as
/// stored, over grey) and preview.png (tinted as in the game, over sky and
/// meadow colours). Returns the files written.
#[cfg(test)]
pub fn export(dir: &std::path::Path, overwrite: bool) -> std::io::Result<Vec<String>> {
    std::fs::create_dir_all(dir)?;
    let mut written = Vec::new();
    let n = layer::COUNT as usize;
    let cell = SIZE + 16;
    let (sw, sh) = (cell * n, cell);
    let mut sheet = vec![0u8; sw * sh * 4];
    let mut preview = vec![0u8; sw * sh * 4];
    for l in 0..n {
        let img = layer_image(l as u32);
        let mut bytes = vec![0u8; SIZE * SIZE * 4];
        for (i, p) in img.iter().enumerate() {
            for k in 0..3 {
                bytes[i * 4 + k] = to_srgb(p[k]);
            }
            bytes[i * 4 + 3] = (p[3].clamp(0.0, 1.0) * 255.0).round() as u8;
        }
        let path = dir.join(format!("{}.png", NAMES[l]));
        if overwrite || !path.exists() {
            write_png(&path, SIZE as u32, SIZE as u32, &bytes)?;
            written.push(path.display().to_string());
        }
        let tinted = PREVIEW_TINT[l][0] < 1.0;
        for y in 0..sh {
            for x in 0..cell {
                let d = (y * sw + l * cell + x) * 4;
                // Backdrops: mid grey for the sheet; sky above meadow for the preview.
                let back = if y < sh * 3 / 4 { [0.45, 0.62, 0.80] } else { [0.20, 0.32, 0.08] };
                let (ix, iy) = (x as isize - 8, y as isize - 8);
                let p = if (0..SIZE as isize).contains(&ix) && (0..SIZE as isize).contains(&iy) {
                    img[iy as usize * SIZE + ix as usize]
                } else {
                    [0.0; 4]
                };
                let a = p[3].clamp(0.0, 1.0);
                for k in 0..3 {
                    sheet[d + k] = to_srgb(p[k] * a + 0.19 * (1.0 - a));
                    let lit = if tinted { (p[k] * PREVIEW_TINT[l][k] * 1.3).min(1.0) } else { p[k] };
                    preview[d + k] = to_srgb(lit * a + back[k] * (1.0 - a));
                }
                sheet[d + 3] = 255;
                preview[d + 3] = 255;
            }
        }
    }
    for (name, data) in [("sheet.png", &sheet), ("preview.png", &preview)] {
        let path = dir.join(name);
        write_png(&path, sw as u32, sh as u32, data)?;
        written.push(path.display().to_string());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_has_something_and_mips_keep_coverage() {
        let mips = build();
        assert_eq!(mips[0].len(), layer::COUNT as usize);
        for l in 0..layer::COUNT as usize {
            let opaque = |m: usize| {
                let d = &mips[m][l];
                d.chunks(4).filter(|p| p[3] >= 128).count() as f32 / (d.len() / 4) as f32
            };
            let c0 = opaque(0);
            assert!(c0 > 0.03, "layer {l} covers {c0}");
            let c3 = opaque(3);
            assert!(c3 > c0 * 0.7, "layer {l}: mip 3 covers {c3}, full size {c0}");
        }
    }

    /// Writes library/sprites: cargo test --release export_sprites -- --ignored
    /// (SPRITES_OVERWRITE=1 replaces existing PNGs with the drawn ones;
    /// SPRITES_OUT=DIR writes somewhere else instead, which leaves the game's
    /// own sprites alone -- what to use to look at the whole set).
    #[test]
    #[ignore]
    fn export_sprites() {
        let over = std::env::var("SPRITES_OVERWRITE").is_ok();
        let dir = std::env::var("SPRITES_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| sprite_dir());
        for f in export(&dir, over).unwrap() {
            println!("wrote {f}");
        }
    }
}
