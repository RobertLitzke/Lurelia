//! The HUD's type: a distance-field atlas of Cinzel and Cormorant Garamond
//! Italic baked by `library/fonts/bake_atlas.py`, and the Lurelia logo. Both
//! are compiled in, so headless tools and tests see the same text the window
//! does.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const ATLAS_PNG: &[u8] = include_bytes!("../../library/fonts/atlas.png");
const ATLAS_JSON: &str = include_str!("../../library/fonts/atlas.json");
const LOGO_PNG: &[u8] = include_bytes!("../../library/logo/lurelia-black-outline.png");

/// The logo's four-point star (in face Caps).
pub const STAR: char = '\u{2726}';

/// Which face a run of text is set in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Face {
    /// Cinzel 600: menus, headings, in-game lines.
    Caps = 0,
    /// Cinzel 700: the selected menu item, headings.
    CapsBold = 1,
    /// Cormorant Garamond Italic 500: notes and sentences.
    Italic = 2,
}

/// One glyph's cell in the atlas (atlas pixels) and how it sits on the pen;
/// distances in atlas pixels at `Font::em` pixels per em.
#[derive(Clone, Copy, Default, Debug)]
pub struct Glyph {
    pub adv: f32,
    pub cell: [u16; 4],
    pub ox: f32,
    pub oy: f32,
}

pub struct FaceMetrics {
    /// Capital height as a fraction of the em.
    pub cap: f32,
    pub glyphs: HashMap<char, Glyph>,
}

pub struct Font {
    /// Atlas pixels per em.
    pub em: f32,
    /// Distance range each side of an edge, atlas pixels.
    pub spread: f32,
    pub width: u32,
    pub height: u32,
    pub faces: Vec<FaceMetrics>,
}

#[derive(Deserialize)]
struct GlyphDef {
    adv: f32,
    #[serde(default)]
    x: u16,
    #[serde(default)]
    y: u16,
    #[serde(default)]
    w: u16,
    #[serde(default)]
    h: u16,
    #[serde(default)]
    ox: f32,
    #[serde(default)]
    oy: f32,
}

#[derive(Deserialize)]
struct FaceDef {
    cap: f32,
    glyphs: HashMap<String, GlyphDef>,
}

#[derive(Deserialize)]
struct AtlasDef {
    em: f32,
    spread: f32,
    width: u32,
    height: u32,
    faces: Vec<FaceDef>,
}

pub fn font() -> &'static Font {
    static F: OnceLock<Font> = OnceLock::new();
    F.get_or_init(|| {
        let def: AtlasDef = serde_json::from_str(ATLAS_JSON).expect("library/fonts/atlas.json");
        let faces = def
            .faces
            .into_iter()
            .map(|f| FaceMetrics {
                cap: f.cap / def.em,
                glyphs: f
                    .glyphs
                    .into_iter()
                    .filter_map(|(k, g)| {
                        let ch = char::from_u32(k.parse().ok()?)?;
                        Some((
                            ch,
                            Glyph {
                                adv: g.adv,
                                cell: [g.x, g.y, g.w, g.h],
                                ox: g.ox,
                                oy: g.oy,
                            },
                        ))
                    })
                    .collect(),
            })
            .collect();
        Font {
            em: def.em,
            spread: def.spread,
            width: def.width,
            height: def.height,
            faces,
        }
    })
}

impl Font {
    pub fn face(&self, f: Face) -> &FaceMetrics {
        &self.faces[f as usize]
    }
}

impl FaceMetrics {
    /// The glyph for `ch`, falling back to '?' for anything not baked.
    pub fn glyph(&self, ch: char) -> Glyph {
        let ch = match ch {
            '\u{2018}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            c => c,
        };
        self.glyphs.get(&ch).or_else(|| self.glyphs.get(&'?')).copied().unwrap_or_default()
    }
}

fn decode(png_bytes: &[u8]) -> (Vec<u8>, u32, u32, usize) {
    let mut dec = png::Decoder::new(std::io::Cursor::new(png_bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().expect("png");
    let mut buf = vec![0; r.output_buffer_size().expect("png size")];
    let info = r.next_frame(&mut buf).expect("png frame");
    let ch = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        png::ColorType::GrayscaleAlpha => 2,
        _ => 1,
    };
    buf.truncate(info.width as usize * info.height as usize * ch);
    (buf, info.width, info.height, ch)
}

/// A full mip chain of `ch`-channel u8 pixels, box-filtered, concatenated
/// largest first, and the number of levels.
fn mips(base: Vec<u8>, w: u32, h: u32, ch: usize) -> (Vec<u8>, u32) {
    let mut out = base.clone();
    let (mut cur, mut cw, mut chh) = (base, w as usize, h as usize);
    let mut levels = 1;
    while cw > 1 || chh > 1 {
        let (nw, nh) = ((cw / 2).max(1), (chh / 2).max(1));
        let mut next = vec![0u8; nw * nh * ch];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..ch {
                    let mut s = 0u32;
                    for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let sx = (x * 2 + dx).min(cw - 1);
                        let sy = (y * 2 + dy).min(chh - 1);
                        s += cur[(sy * cw + sx) * ch + c] as u32;
                    }
                    next[(y * nw + x) * ch + c] = ((s + 2) / 4) as u8;
                }
            }
        }
        out.extend_from_slice(&next);
        cur = next;
        cw = nw;
        chh = nh;
        levels += 1;
    }
    (out, levels)
}

/// The atlas as R8 with mips: (data, width, height, levels).
pub fn atlas_mips() -> (Vec<u8>, u32, u32, u32) {
    let (px, w, h, ch) = decode(ATLAS_PNG);
    let grey: Vec<u8> = if ch == 1 { px } else { px.chunks(ch).map(|p| p[0]).collect() };
    let (data, levels) = mips(grey, w, h, 1);
    (data, w, h, levels)
}

/// The logo as premultiplied sRGB RGBA with mips: (data, width, height, levels).
/// Premultiplied so filtering doesn't bleed the colour of see-through pixels;
/// the shader divides it back out and converts to linear.
pub fn logo_mips() -> (Vec<u8>, u32, u32, u32) {
    let (px, w, h, ch) = decode(LOGO_PNG);
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for p in px.chunks(ch) {
        let a = if ch == 4 { p[3] as u32 } else { 255 };
        for c in 0..3 {
            rgba.push(((p[c.min(ch - 1)] as u32 * a + 127) / 255) as u8);
        }
        rgba.push(a as u8);
    }
    let (data, levels) = mips(rgba, w, h, 4);
    (data, w, h, levels)
}

/// The logo's size in pixels.
pub fn logo_size() -> (u32, u32) {
    static S: OnceLock<(u32, u32)> = OnceLock::new();
    *S.get_or_init(|| {
        let dec = png::Decoder::new(std::io::Cursor::new(LOGO_PNG));
        let r = dec.read_info().expect("logo png");
        (r.info().width, r.info().height)
    })
}

/// sRGB hex (0xRRGGBB) to the linear colour the HUD draws with.
pub fn hex(rgb: u32, a: f32) -> [f32; 4] {
    let lin = |c: u32| {
        let s = c as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [lin((rgb >> 16) & 255), lin((rgb >> 8) & 255), lin(rgb & 255), a]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atlas_has_every_face_and_the_star() {
        let f = font();
        assert_eq!(f.faces.len(), 3);
        for face in &f.faces {
            for ch in ('A'..='Z').chain('0'..='9') {
                assert!(face.glyphs.contains_key(&ch), "missing {ch}");
            }
            assert!(face.cap > 0.5 && face.cap < 0.8, "cap {}", face.cap);
        }
        assert!(f.face(Face::Caps).glyphs.contains_key(&STAR));
        let (_, w, h, levels) = atlas_mips();
        assert_eq!((w, h), (f.width, f.height));
        assert!(levels > 5);
    }
}
