//! Named places: the areas a world calls something, and the signs that say so
//! on the ground.
//!
//! An area is a polygon with a name. Walk far enough into one and the HUD
//! names it in the corner (`game.rs`), and the gazetteer lists them, so an
//! author and a player can refer to the same ground by the same word. Areas
//! may overlap and nest: the smallest one you are properly inside wins, so a
//! street inside a district inside a city names the street.
//!
//! A sign is a board with a line or two of text on it, standing at a bearing
//! anywhere in the world. Its board and its lettering are drawn as plates
//! (`render/plates.rs`), not voxels, so a street blade can be 20 cm tall and
//! still read; whatever holds it up (a post, a plinth, a cairn) is an ordinary
//! library structure placed under it.

use crate::render::font::{font, Face};
use crate::render::plates::Plates;
use glam::{Vec2, Vec3};
use serde::Deserialize;

/// How a sign's board and lettering look: a library entry under
/// `library/signs/`.
#[derive(Deserialize, Debug, Clone)]
pub struct SignStyle {
    pub name: String,
    #[serde(default = "one")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    #[serde(default)]
    #[allow(dead_code)] // for an author reading the library
    pub notes: String,
    /// The structure that holds the board up (a post, a pair of posts, a
    /// plinth), placed at the sign's foot. None for a board on a wall.
    #[serde(default)]
    pub post: Option<String>,
    /// Board size in metres: width, height.
    pub board: [f32; 2],
    /// Height of the board's centre above the ground, metres.
    #[serde(default = "default_centre")]
    pub centre_m: f32,
    /// How thick the board is, metres.
    #[serde(default = "default_thickness")]
    pub thickness: f32,
    /// How far the back of the board stands clear of whatever holds it up,
    /// metres. A couple of centimetres for a board nailed to a post; more for
    /// one hung on the face of a structure that is not flat.
    #[serde(default)]
    pub offset_m: f32,
    /// Board colour, sRGB bytes.
    pub face: [u8; 3],
    /// The board's edges and its painted border, sRGB bytes.
    pub edge: [u8; 3],
    /// The lettering, sRGB bytes.
    pub ink: [u8; 3],
    /// Cap height of the lettering, metres. 0 fits the text to the board.
    #[serde(default)]
    pub cap_m: f32,
    /// A painted border this far in from the board's edge; 0 for none.
    #[serde(default)]
    pub border_m: f32,
    /// "caps" (the default, Cinzel) or "italic" (Cormorant Garamond Italic).
    #[serde(default)]
    pub letters: String,
    /// Whether the back of the board carries the text as well.
    #[serde(default = "yes")]
    pub two_sided: bool,
    /// No board at all: the letters are cut into whatever holds them (a
    /// stone, a wall), so only the lettering is drawn. `board` is then the
    /// space the words are set in.
    #[serde(default)]
    pub bare: bool,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}
fn default_centre() -> f32 {
    2.0
}
fn default_thickness() -> f32 {
    0.08
}

impl SignStyle {
    pub fn face_kind(&self) -> Face {
        if self.letters == "italic" {
            Face::Italic
        } else {
            Face::Caps
        }
    }
}

/// A sign standing in the world.
#[derive(Debug, Clone)]
pub struct Sign {
    /// Where the board's centre is, metres.
    pub at: Vec3,
    /// Compass bearing the front of the board faces.
    pub bearing: f32,
    /// Index into `Library::signs`.
    pub style: usize,
    /// One string a line, top to bottom.
    pub text: Vec<String>,
}

/// Past this far away a sign's board is not drawn at all.
pub const BOARD_RANGE: f32 = 140.0;
/// Past this far away its lettering is not drawn.
pub const TEXT_RANGE: f32 = 64.0;

fn linear(c: [u8; 3], a: f32) -> [f32; 4] {
    let f = |v: u8| {
        let s = v as f32 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), a]
}

impl Sign {
    /// The sign's own axes: `u` across the board to the right of a reader
    /// facing it, `v` up, `n` out of its front.
    pub fn axes(&self) -> (Vec3, Vec3, Vec3) {
        // Bearing 0 faces north (-Z) and grows toward the west. `u` runs the
        // way a reader standing in front of the board reads, which is to
        // their right: facing south, that is west.
        let r = self.bearing.to_radians();
        let n = Vec3::new(r.sin(), 0.0, -r.cos());
        let u = Vec3::new(n.z, 0.0, -n.x);
        (u, Vec3::Y, n)
    }

    /// Draw the board and, close enough to read, the words on it.
    pub fn draw(&self, style: &SignStyle, cam: Vec3, out: &mut Plates) {
        let d = self.at.distance(cam);
        if d > BOARD_RANGE {
            return;
        }
        let (u, v, n) = self.axes();
        let (hw, hh) = (style.board[0] * 0.5, style.board[1] * 0.5);
        let t = style.thickness * 0.5;
        let fade = |near: f32| ((near - d) / (near * 0.2)).clamp(0.0, 1.0);
        let a = fade(BOARD_RANGE);
        let face = linear(style.face, a);
        let edge = linear(style.edge, a);
        // The board: two faces and four edges.
        if !style.bare {
            out.flat(self.at + n * t, u * hw, v * hh, face);
            out.flat(self.at - n * t, -u * hw, v * hh, face);
            out.flat(self.at + v * hh, u * hw, n * t, edge);
            out.flat(self.at - v * hh, u * hw, n * t, edge);
            out.flat(self.at + u * hw, n * t, v * hh, edge);
            out.flat(self.at - u * hw, n * t, v * hh, edge);
            // A painted border, a hair in front of each face.
            if style.border_m > 0.0 {
                let b = style.border_m;
                let w = (b * 0.35).max(0.012);
                for (side, uu) in [(n, u), (-n, -u)] {
                    if side.dot(n) < 0.0 && !style.two_sided {
                        continue;
                    }
                    let o = self.at + side * (t + 0.004);
                    let (iw, ih) = (hw - b, hh - b);
                    if iw <= w || ih <= w {
                        continue;
                    }
                    out.flat(o + v * ih, uu * iw, v * w, edge);
                    out.flat(o - v * ih, uu * iw, v * w, edge);
                    out.flat(o + uu * iw, uu * w, v * ih, edge);
                    out.flat(o - uu * iw, uu * w, v * ih, edge);
                }
            }
        }
        if d > TEXT_RANGE || self.text.is_empty() {
            return;
        }
        let ink = linear(style.ink, fade(TEXT_RANGE));
        for (side, uu) in [(n, u), (-n, -u)] {
            if side.dot(n) < 0.0 && !style.two_sided {
                continue;
            }
            // Only the face turned toward the camera is worth the plates.
            if side.dot(cam - self.at) <= 0.0 {
                continue;
            }
            self.letters(style, self.at + side * (t + 0.008), uu, v, ink, out);
        }
    }

    /// Set the sign's lines on one face, centred in the space inside the
    /// border.
    fn letters(&self, style: &SignStyle, origin: Vec3, u: Vec3, v: Vec3, ink: [f32; 4], out: &mut Plates) {
        let f = font();
        let face = f.face(style.face_kind());
        let lines: Vec<&str> = self.text.iter().map(|s| s.as_str()).collect();
        let em = fit_em(style, &lines);
        let track = TRACK;
        let width_em = |s: &str| line_width_em(style, s);
        let leading = LEADING;
        let stack = face.cap + leading * (lines.len() - 1) as f32;
        let step = leading * em;
        // Baselines, top line first, the block centred on the board.
        let top = (stack * em) * 0.5 - face.cap * em;
        for (i, line) in lines.iter().enumerate() {
            let base = top - i as f32 * step;
            let mut x = -width_em(line) * em * 0.5;
            for ch in line.chars() {
                let g = face.glyph(ch);
                let k = em / f.em;
                if ch != ' ' && g.cell[2] > 0 {
                    let (gw, gh) = (g.cell[2] as f32 * k, g.cell[3] as f32 * k);
                    let cu = x + (g.ox + g.cell[2] as f32 * 0.5) * k;
                    let cv = base - (g.oy + g.cell[3] as f32 * 0.5) * k;
                    out.glyph(
                        origin + u * cu + v * cv,
                        u * gw * 0.5,
                        v * gh * 0.5,
                        ink,
                        [g.cell[0] as u32, g.cell[1] as u32, g.cell[2] as u32, g.cell[3] as u32],
                    );
                }
                x += g.adv * k + track * em;
            }
        }
    }
}

/// Letter spacing, ems.
const TRACK: f32 = 0.03;
/// Line to line, ems.
const LEADING: f32 = 1.42;

/// How wide a line of text is, in ems of the style's face.
fn line_width_em(style: &SignStyle, line: &str) -> f32 {
    let f = font();
    let face = f.face(style.face_kind());
    let n = line.chars().count().max(1) - 1;
    line.chars().map(|c| face.glyph(c).adv).sum::<f32>() / f.em + TRACK * n as f32
}

/// Metres per em for these lines on this style's board: the style's own cap
/// height, or smaller where the words need the room.
pub fn fit_em(style: &SignStyle, lines: &[&str]) -> f32 {
    let face = font().face(style.face_kind());
    let inset = style.border_m * 2.0 + 0.03;
    let room = Vec2::new((style.board[0] - inset * 2.0).max(0.02), (style.board[1] - inset * 2.0).max(0.02));
    let widest = lines.iter().map(|s| line_width_em(style, s)).fold(0.01, f32::max);
    let stack = face.cap + LEADING * (lines.len() - 1) as f32;
    let mut em = (room.x / widest).min(room.y / stack);
    if style.cap_m > 0.0 {
        em = em.min(style.cap_m / face.cap);
    }
    em
}

/// The cap height the words come out at, metres: what the style asks for, or
/// less when the lines are too long for the board.
pub fn fit_cap(style: &SignStyle, lines: &[&str]) -> f32 {
    fit_em(style, lines) * font().face(style.face_kind()).cap
}

/// A named stretch of ground.
#[derive(Debug, Clone)]
pub struct Area {
    pub name: String,
    /// What kind of place it is ("village", "wood", "street"), for the author;
    /// the HUD shows the name alone.
    pub kind: String,
    /// The outline, metres, in order; at least three points.
    pub poly: Vec<Vec2>,
    /// How far inside the outline you must be before the HUD names it.
    pub inside_m: f32,
    /// Bounding box, for a quick reject.
    pub lo: Vec2,
    pub hi: Vec2,
    /// How big it is, so the smallest area containing a point wins.
    pub area_m2: f32,
}

impl Area {
    pub fn new(name: String, kind: String, poly: Vec<Vec2>, inside_m: f32) -> Area {
        let mut lo = Vec2::splat(f32::MAX);
        let mut hi = Vec2::splat(f32::MIN);
        let mut twice = 0.0;
        for (i, p) in poly.iter().enumerate() {
            lo = lo.min(*p);
            hi = hi.max(*p);
            let q = poly[(i + 1) % poly.len()];
            twice += p.x * q.y - q.x * p.y;
        }
        Area {
            name,
            kind,
            poly,
            inside_m,
            lo,
            hi,
            area_m2: twice.abs() * 0.5,
        }
    }

    /// A ragged circle as a polygon, for a village or a wood with no mapped
    /// outline: `wobble` is how much the radius varies.
    pub fn disc(name: String, kind: String, at: Vec2, radius: f32, wobble: f32, seed: u32) -> Area {
        let n = 24;
        let poly = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let r = radius * (1.0 + wobble * (crate::world::noise::unit(crate::world::noise::hash32(seed ^ i as u32 * 0x9e37)) - 0.5) * 2.0);
                at + Vec2::new(a.cos(), a.sin()) * r
            })
            .collect();
        Area::new(name, kind, poly, 0.0)
    }

    /// Is the point inside the outline (a crossing count)?
    pub fn contains(&self, p: Vec2) -> bool {
        if p.x < self.lo.x || p.x > self.hi.x || p.y < self.lo.y || p.y > self.hi.y {
            return false;
        }
        let mut inside = false;
        let n = self.poly.len();
        for i in 0..n {
            let (a, b) = (self.poly[i], self.poly[(i + 1) % n]);
            if (a.y > p.y) != (b.y > p.y) {
                let t = (p.y - a.y) / (b.y - a.y);
                if a.x + t * (b.x - a.x) > p.x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Distance from the point to the outline, metres (positive either side).
    pub fn edge_distance(&self, p: Vec2) -> f32 {
        let n = self.poly.len();
        let mut best = f32::MAX;
        for i in 0..n {
            let (a, b) = (self.poly[i], self.poly[(i + 1) % n]);
            let ab = b - a;
            let t = if ab.length_squared() < 1e-6 { 0.0 } else { (p - a).dot(ab) / ab.length_squared() };
            best = best.min(p.distance(a + ab * t.clamp(0.0, 1.0)));
        }
        best
    }

    /// How far inside the outline the point is: negative outside.
    pub fn depth(&self, p: Vec2) -> f32 {
        let d = self.edge_distance(p);
        if self.contains(p) {
            d
        } else {
            -d
        }
    }
}

/// The area that names where `p` is: of the areas you are `inside_m` or more
/// inside, the smallest. `was` is the area named a moment ago (by index); it
/// keeps its claim until you are `inside_m` back outside it, so standing on a
/// boundary doesn't flicker.
pub fn naming(areas: &[Area], p: Vec2, was: Option<usize>) -> Option<usize> {
    if let Some(k) = was {
        if areas.get(k).is_some_and(|a| a.depth(p) > -a.inside_m.max(1.0)) {
            // Still in it, unless something smaller now claims the ground.
            let smaller = areas
                .iter()
                .enumerate()
                .filter(|(i, a)| *i != k && a.area_m2 < areas[k].area_m2 && a.depth(p) >= a.inside_m)
                .min_by(|a, b| a.1.area_m2.partial_cmp(&b.1.area_m2).unwrap());
            return Some(smaller.map_or(k, |(i, _)| i));
        }
    }
    areas
        .iter()
        .enumerate()
        .filter(|(_, a)| a.depth(p) >= a.inside_m)
        .min_by(|a, b| a.1.area_m2.partial_cmp(&b.1.area_m2).unwrap())
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(name: &str, lo: f32, hi: f32, inside: f32) -> Area {
        Area::new(name.into(), "test".into(), vec![Vec2::new(lo, lo), Vec2::new(hi, lo), Vec2::new(hi, hi), Vec2::new(lo, hi)], inside)
    }

    #[test]
    fn depth_and_containment() {
        let a = square("a", 0.0, 100.0, 10.0);
        assert!(a.contains(Vec2::new(50.0, 50.0)));
        assert!(!a.contains(Vec2::new(-1.0, 50.0)));
        assert!((a.depth(Vec2::new(50.0, 50.0)) - 50.0).abs() < 0.01);
        assert!((a.depth(Vec2::new(5.0, 50.0)) - 5.0).abs() < 0.01);
        assert!((a.depth(Vec2::new(-5.0, 50.0)) + 5.0).abs() < 0.01);
        assert!((a.area_m2 - 10000.0).abs() < 1.0);
    }

    /// The smallest area wins, and the name holds on past the boundary so a
    /// step back and forth doesn't blink it.
    #[test]
    fn naming_prefers_the_smaller_and_holds() {
        let areas = vec![square("district", 0.0, 200.0, 8.0), square("green", 40.0, 80.0, 4.0)];
        assert_eq!(naming(&areas, Vec2::new(100.0, 100.0), None), Some(0));
        assert_eq!(naming(&areas, Vec2::new(60.0, 60.0), None), Some(1));
        // Just outside the green: nothing has claimed it, so the district does.
        assert_eq!(naming(&areas, Vec2::new(82.0, 60.0), None), Some(0));
        // Having just been in the green, it keeps the name a few metres out.
        assert_eq!(naming(&areas, Vec2::new(82.0, 60.0), Some(1)), Some(1));
        // Well out of it, the district takes over again.
        assert_eq!(naming(&areas, Vec2::new(95.0, 60.0), Some(1)), Some(0));
        // Off the map entirely: no name.
        assert_eq!(naming(&areas, Vec2::new(-50.0, -50.0), Some(0)), None);
    }

    /// A line of text is laid out inside the board, and the plates it makes
    /// stay on the board's face.
    #[test]
    fn sign_letters_fit_the_board() {
        let style = SignStyle {
            name: "test".into(),
            version: 1,
            introduced_by: String::new(),
            notes: String::new(),
            post: None,
            board: [2.0, 0.5],
            centre_m: 2.0,
            thickness: 0.08,
            offset_m: 0.05,
            face: [40, 60, 40],
            edge: [240, 240, 240],
            ink: [240, 240, 240],
            cap_m: 0.0,
            border_m: 0.04,
            letters: String::new(),
            two_sided: true,
            bare: false,
        };
        let sign = Sign {
            at: Vec3::new(10.0, 3.0, 10.0),
            bearing: 0.0,
            style: 0,
            text: vec!["FREMONT AVE".into()],
        };
        let mut p = Plates::default();
        sign.draw(&style, Vec3::new(10.0, 3.0, 4.0), &mut p);
        assert!(p.len() > 20, "board, border and letters: {}", p.len());
        for pl in p.packed() {
            let (dx, dy) = (pl.pos[0] - 10.0, pl.pos[1] - 3.0);
            assert!(dx.abs() <= 1.01 && dy.abs() <= 0.26, "plate outside the board: {dx} {dy}");
        }
    }
}
