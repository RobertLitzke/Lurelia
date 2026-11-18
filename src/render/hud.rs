//! On-screen text and shapes: a list of instanced quads in pixel coordinates.
//! Text is set from the distance-field atlas in font.rs, so it stays sharp at
//! any size; `text`/`label` keep the old size unit (`s`, pixels per pixel of
//! the original 5x7 font) so callers that lay out with it still fit.

use super::font::{font, Face, STAR};
use bytemuck::{Pod, Zeroable};

/// A plain filled rectangle.
pub const SOLID: u32 = 127;
/// Draws the image (see IMAGE_PX) over the whole rect instead of a font cell.
pub const MAP: u32 = 1000;
/// Side of the square image glyph MAP shows (a mode's full-screen page,
/// such as a scrapbook page).
pub const IMAGE_PX: usize = 1024;
/// Draws backdrop picture 0 over the whole rect; BACKDROP + 1 draws picture 1.
/// The menus cross-fade screenshots with the two.
pub const BACKDROP: u32 = 1001;
/// The Lurelia logo over the whole rect.
pub const LOGO: u32 = 1003;
/// A glyph from the font atlas: `_pad` holds its cell and edge softness.
pub const TEXT: u32 = 2000;
/// A rectangle shading from `color` at the left (GRAD_H) or top (GRAD_V) to
/// the colour packed in `_pad[0]` at the other side.
pub const GRAD_H: u32 = 3000;
pub const GRAD_V: u32 = 3001;
/// Size of each backdrop picture (16:9; menus.rs crops screenshots to fit).
pub const BACKDROP_W: u32 = 1920;
pub const BACKDROP_H: u32 = 1080;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct HudInst {
    pub rect: [f32; 4],
    pub color: [f32; 4],
    pub glyph: u32,
    pub _pad: [u32; 3],
}

/// How a run of text is set: face, size in pixels per em, and extra letter
/// spacing in ems.
#[derive(Clone, Copy, Debug)]
pub struct Type {
    pub face: Face,
    pub size: f32,
    pub track: f32,
}

impl Type {
    pub fn new(face: Face, size: f32, track: f32) -> Type {
        Type { face, size, track }
    }

    /// Capital height in pixels.
    pub fn cap(&self) -> f32 {
        font().face(self.face).cap * self.size
    }
}

/// Text colour: one colour, or a gradient left to right across the run.
#[derive(Clone, Copy)]
pub enum Ink<'a> {
    Flat([f32; 4]),
    Grad(&'a [[f32; 4]]),
}

/// The old font's size unit: the em and letter spacing that keep a line about
/// as wide and as tall as it was.
fn old_type(s: f32) -> Type {
    Type::new(Face::Caps, 8.2 * s, 0.03)
}

fn pack_rgba(c: [f32; 4]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    b(c[0]) | b(c[1]) << 8 | b(c[2]) << 16 | b(c[3]) << 24
}

#[derive(Default)]
pub struct Hud {
    pub items: Vec<HudInst>,
}

/// `c` with its alpha replaced by `a`.
pub fn with_a(c: [f32; 4], a: f32) -> [f32; 4] {
    [c[0], c[1], c[2], a]
}

impl Hud {
    pub fn clear(&mut self) {
        self.items.clear();
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color,
            glyph: SOLID,
            _pad: [0; 3],
        });
    }

    /// A rectangle shading from `a` at the left to `b` at the right.
    pub fn grad_h(&mut self, x: f32, y: f32, w: f32, h: f32, a: [f32; 4], b: [f32; 4]) {
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color: a,
            glyph: GRAD_H,
            _pad: [pack_rgba(b), 0, 0],
        });
    }

    /// A rectangle shading from `a` at the top to `b` at the bottom.
    pub fn grad_v(&mut self, x: f32, y: f32, w: f32, h: f32, a: [f32; 4], b: [f32; 4]) {
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color: a,
            glyph: GRAD_V,
            _pad: [pack_rgba(b), 0, 0],
        });
    }

    /// The CPU-drawn page image (render/mod.rs `upload_map`) over the rect.
    pub fn page(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color: [1.0; 4],
            glyph: MAP,
            _pad: [0; 3],
        });
    }

    /// Backdrop picture `slot` (0 or 1) over the rect, faded by `alpha`.
    pub fn backdrop(&mut self, slot: usize, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color: [1.0, 1.0, 1.0, alpha],
            glyph: BACKDROP + slot as u32,
            _pad: [0; 3],
        });
    }

    /// The logo with its top-left at (x, y), `w` pixels wide.
    pub fn logo(&mut self, x: f32, y: f32, w: f32, alpha: f32) {
        let (lw, lh) = super::font::logo_size();
        let h = w * lh as f32 / lw as f32;
        self.items.push(HudInst {
            rect: [x, y, w, h],
            color: [1.0, 1.0, 1.0, alpha],
            glyph: LOGO,
            _pad: [0; 3],
        });
    }

    /// One atlas glyph; `soft` > 0 grows and blurs it by that many pixels
    /// (for halos behind text).
    fn glyph(&mut self, ch: char, face: Face, size: f32, x: f32, baseline: f32, color: [f32; 4], soft: f32) {
        let f = font();
        let g = f.face(face).glyph(ch);
        if g.cell[2] == 0 {
            return;
        }
        let k = size / f.em;
        self.items.push(HudInst {
            rect: [x + g.ox * k, baseline + g.oy * k, g.cell[2] as f32 * k, g.cell[3] as f32 * k],
            color,
            glyph: TEXT,
            _pad: [g.cell[0] as u32 | (g.cell[1] as u32) << 16, g.cell[2] as u32 | (g.cell[3] as u32) << 16, soft.to_bits()],
        });
    }

    /// Width of `text` set in `t`, without spacing after the last letter.
    pub fn measure(t: Type, text: &str) -> f32 {
        let f = font();
        let face = f.face(t.face);
        let k = t.size / f.em;
        let mut w = 0.0;
        let mut n = 0;
        for ch in text.chars() {
            w += face.glyph(ch).adv * k;
            n += 1;
        }
        w + t.track * t.size * (n.max(1) - 1) as f32
    }

    /// Break `text` into lines no wider than `width`, set in `t`. A word
    /// wider than the width gets a line to itself rather than being cut.
    pub fn wrap(t: Type, text: &str, width: f32) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut line = String::new();
        for word in text.split_whitespace() {
            let next = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if !line.is_empty() && Self::measure(t, &next) > width {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = next;
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
        out
    }

    fn run(&mut self, t: Type, x: f32, baseline: f32, ink: Ink, soft: f32, text: &str) -> f32 {
        let f = font();
        let face = f.face(t.face);
        let k = t.size / f.em;
        let total = Self::measure(t, text).max(1.0);
        let mut cx = x;
        for ch in text.chars() {
            let adv = face.glyph(ch).adv * k;
            let color = match ink {
                Ink::Flat(c) => c,
                Ink::Grad(stops) => {
                    // The stop colour at the middle of this letter.
                    let u = ((cx + adv * 0.5 - x) / total).clamp(0.0, 1.0) * (stops.len() - 1) as f32;
                    let i = (u.floor() as usize).min(stops.len().saturating_sub(2));
                    let fr = u - i as f32;
                    let (a, b) = (stops[i], stops[(i + 1).min(stops.len() - 1)]);
                    std::array::from_fn(|j| a[j] + (b[j] - a[j]) * fr)
                }
            };
            if ch != ' ' {
                self.glyph(ch, t.face, t.size, cx, baseline, color, soft);
            }
            cx += adv + t.track * t.size;
        }
        cx - x - t.track * t.size
    }

    /// Set `text` with its baseline at `baseline`; returns the width.
    pub fn set(&mut self, t: Type, x: f32, baseline: f32, ink: Ink, text: &str) -> f32 {
        self.run(t, x, baseline, ink, 0.0, text)
    }

    /// A soft dark halo behind text set at the same place, so it reads over
    /// bright ground without a box. `alpha` is the halo's strength.
    pub fn halo(&mut self, t: Type, x: f32, baseline: f32, alpha: f32, text: &str) {
        let soft = (t.size * 0.18).max(1.5);
        self.run(t, x, baseline, Ink::Flat([0.03, 0.02, 0.03, alpha]), soft, text);
    }

    /// Halo, then the text.
    pub fn set_lit(&mut self, t: Type, x: f32, baseline: f32, ink: Ink, halo: f32, text: &str) -> f32 {
        self.halo(t, x, baseline, halo, text);
        self.set(t, x, baseline, ink, text)
    }

    /// The four-point star centred on (cx, cy), `size` pixels across.
    pub fn star(&mut self, cx: f32, cy: f32, size: f32, color: [f32; 4]) {
        self.glyph(STAR, Face::Caps, size, cx, cy, color, 0.0);
    }

    /// Text in the in-game face with its top-left at (x, y); `s` is the old
    /// font's pixels per pixel. Returns the width drawn.
    pub fn text(&mut self, x: f32, y: f32, s: f32, color: [f32; 4], text: &str) -> f32 {
        let t = old_type(s);
        self.set(t, x, y + 6.4 * s, Ink::Flat(color), text)
    }

    pub fn text_width(text: &str, s: f32) -> f32 {
        Self::measure(old_type(s), text)
    }

    /// Text with a soft dark halo, readable on any background.
    pub fn label(&mut self, x: f32, y: f32, s: f32, color: [f32; 4], text: &str) -> f32 {
        let t = old_type(s);
        self.halo(t, x, y + 6.4 * s, color[3] * 0.5, text);
        self.text(x, y, s, color, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Old-unit text stays within about a third of the 5x7 font's width, so
    /// layouts built on it still fit.
    #[test]
    fn old_unit_width_is_close() {
        let s = 2.0;
        let line = "CLICK SHUTTER   RIGHT CLICK FRAME   WHEEL ZOOM";
        let old = line.chars().count() as f32 * 6.0 * s;
        let new = Hud::text_width(line, s);
        assert!(new > old * 0.7 && new < old * 1.35, "old {old} new {new}");
    }

    #[test]
    fn measure_matches_set() {
        let mut h = Hud::default();
        let t = Type::new(Face::Italic, 30.0, 0.02);
        let w = h.set(t, 10.0, 50.0, Ink::Flat([1.0; 4]), "Glassfall");
        assert!((w - Hud::measure(t, "Glassfall")).abs() < 0.01);
        assert_eq!(h.items.len(), 9);
    }
}
