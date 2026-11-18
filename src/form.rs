//! The two editing pages as forms: the LOOK page (F7, src/lookpage.rs) down
//! the left, the EDIT page (F8, src/editor) down the right.
//!
//! The menu style -- a column of lit words with a caret beside the chosen one
//! (`menu::draw_menu`) -- reads well for half a dozen choices and badly for
//! sixty fields. A form reads instead: a label on the left, a box on the right
//! holding the value, headings that open and close, the whole of it scrolling
//! under a pinned head and a pinned row of buttons. The panel is solid rather
//! than a fade, so a dark animal in front of a dark hill does not run into
//! the type.
//!
//! The rows carry the same `menu::Item`s the pages already work in, so
//! clicking, stepping and the dropdown anchors go on working as they did:
//! this module only decides where each one is drawn and fills in the hit
//! boxes (`Menu::hits`, `minus`, `plus`, `drops`) the mouse reads.
//!
//! The knobs on both pages are described the same way too (`Knob`, `Num`):
//! what differs is only how a page reaches the value, a pair of functions on
//! the LOOK page and a path into a JSON entry on the EDIT page.

use crate::menu::{self, Item, Menu};
use crate::render::font::Face;
use crate::render::hud::{with_a, Hud, Ink, Type};

// ---- knobs -----------------------------------------------------------------

/// How a number is stepped and how it reads.
#[derive(Clone, Copy)]
pub struct Num {
    pub lo: f32,
    pub hi: f32,
    pub step: f32,
    /// Decimal places shown, and kept: a stepped value is rounded to them, so
    /// it never drifts to 0.4700001.
    pub dp: u8,
    pub unit: Unit,
}

/// What a number is, for reading it.
#[derive(Clone, Copy, PartialEq)]
pub enum Unit {
    Plain,
    /// Metres: "120 m".
    Metres,
    /// A count where nothing means the thing is off: "off" below 1.5.
    Count,
}

impl Num {
    pub const fn new(lo: f32, hi: f32, step: f32, dp: u8) -> Num {
        Num { lo, hi, step, dp, unit: Unit::Plain }
    }

    pub const fn reads(mut self, unit: Unit) -> Num {
        self.unit = unit;
        self
    }

    /// `v` moved `dir` steps, kept in range and rounded to the places shown.
    pub fn nudge(&self, v: f32, dir: i32) -> f32 {
        let v = (v + self.step * dir as f32).clamp(self.lo, self.hi);
        let m = 10f64.powi(self.dp as i32);
        (((v as f64) * m).round() / m) as f32
    }

    pub fn text(&self, v: f32) -> String {
        let dp = self.dp as usize;
        match self.unit {
            Unit::Plain => format!("{v:.dp$}"),
            Unit::Metres => format!("{v:.dp$} m"),
            Unit::Count if v < 1.5 => "off".to_string(),
            Unit::Count => format!("{v:.dp$}"),
        }
    }
}

/// What a knob edits.
#[derive(Clone, Copy)]
pub enum Val {
    Num(Num),
    Flag,
    /// One of a fixed list of names.
    Pick(&'static [&'static str]),
    /// A material name: steps through the library's materials.
    Mat,
    /// An sRGB triple the picker paints.
    Colour,
}

/// One line of an editing page. `R` is how the page reaches the value.
pub struct Knob<R> {
    /// Which group it belongs to (1 up; 0 is the front page).
    pub page: u8,
    pub label: &'static str,
    /// What it does, shown under the list while the line is picked.
    pub help: &'static str,
    pub val: Val,
    pub reach: R,
}

// ---- rows ------------------------------------------------------------------

/// What the field looks like and how it is worked.
#[derive(Clone, Copy, PartialEq)]
pub enum Field {
    /// A heading. Opens and closes the rows under it.
    Section { open: bool },
    /// Stepped with the arrows: a number, or one of a short fixed run.
    Step,
    /// A gradient stop: stepped along its strip like a number, and drawn
    /// with its colour (as the HUD paints it) beside the value.
    Stop([f32; 4]),
    /// Picked from a list that drops under the row.
    List,
    /// On, off, or not set at all.
    Flag(Option<bool>),
    /// A swatch (as the HUD paints it) and its hex; a click opens the picker.
    Colour([f32; 4]),
    /// A line in the button row at the foot.
    Button,
    /// A line that does something, said in a word in a small box on the end
    /// ("ROLL", "OPEN", "WRITE").
    Go(&'static str),
    /// A line with nothing on the end.
    Plain,
}

pub struct Row {
    pub item: Item,
    pub field: Field,
    /// The value is not written in this entry's own file: it comes from a
    /// parent, or from the engine's own default. Drawn dimmed, so what the
    /// file actually says can be read off the panel.
    pub dim: bool,
    /// 0 for the head block and the headings, 1 for a field under a heading.
    pub indent: u8,
}

impl Row {
    pub fn new(item: Item, field: Field) -> Row {
        Row { item, field, dim: false, indent: 0 }
    }

    pub fn under(mut self) -> Row {
        self.indent = 1;
        self
    }

    pub fn dimmed(mut self, dim: bool) -> Row {
        self.dim = dim;
        self
    }
}

// ---- the panel -------------------------------------------------------------

/// The panel's width, in menu units. The EDIT page's camera frames the
/// subject in what is left (editor/cam.rs), so the two belong together.
pub const BAND: f32 = 44.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Left,
    Right,
}

/// What a page's form is made of, beyond its rows. The rows run: `head`
/// pinned at the top, then the scrolling body, then `buttons` pinned at the
/// foot.
pub struct Form<'a> {
    pub title: &'a str,
    pub foot: &'a str,
    pub head: usize,
    pub buttons: usize,
    pub side: Side,
    /// Lines of the picked row's help shown under the body.
    pub help_lines: usize,
    /// The rows tighten, down to four fifths of their height, before the
    /// body scrolls. For a page short enough to want to be seen whole.
    pub squeeze: bool,
}

/// Where the panel's parts sit, so the mouse and the drawing agree.
pub struct Shape {
    pub u: f32,
    pub x0: f32,
    /// The scrolling body.
    pub body: [f32; 4],
    /// Height of one field row.
    pub pitch: f32,
}

pub fn shape(w: f32, h: f32, f: &Form) -> Shape {
    let u = menu::unit(w, h);
    let x0 = match f.side {
        Side::Left => 0.0,
        Side::Right => w - BAND * u,
    };
    // Laid out for a head of two rows; a shorter head gives its room back.
    let top = 13.6 * u - (2.0 - f.head.min(2) as f32) * 2.35 * u;
    let foot = 12.6 * u + (f.help_lines.max(2) - 2) as f32 * 1.65 * u;
    Shape {
        u,
        x0,
        body: [x0, top, BAND * u, (h - foot - top).max(4.0 * u)],
        pitch: (2.35 * u).round().max(9.0),
    }
}

/// A hairline box: four thin rects, which is cheaper than any border the HUD
/// has and crisp at any size.
fn frame(hud: &mut Hud, r: [f32; 4], t: f32, c: [f32; 4]) {
    hud.rect(r[0], r[1], r[2], t, c);
    hud.rect(r[0], r[1] + r[3] - t, r[2], t, c);
    hud.rect(r[0], r[1], t, r[3], c);
    hud.rect(r[0] + r[2] - t, r[1], t, r[3], c);
}

/// A small solid triangle, pointing down when a heading is open and right
/// when it is shut. Drawn as stacked rects, which keeps its edges on pixels.
fn tri(hud: &mut Hud, x: f32, y: f32, s: f32, down: bool, c: [f32; 4]) {
    let n = 4;
    for i in 0..n {
        let k = i as f32 / n as f32;
        let step = (s / n as f32).max(1.0);
        if down {
            hud.rect((x + k * s * 0.5).round(), (y + i as f32 * step).round(), (s * (1.0 - k)).max(1.0), step, c);
        } else {
            hud.rect((x + i as f32 * step).round(), (y + k * s * 0.5).round(), step, (s * (1.0 - k)).max(1.0), c);
        }
    }
}

/// Draw the whole panel. `top` and `shown` are the body's scroll: the first
/// body row drawn, and how many fit (written back for the wheel and keys).
pub fn draw(hud: &mut Hud, m: &mut Menu, rows: &[Row], f: &Form, top: &mut usize, shown: &mut usize, w: f32, h: f32) {
    let mut s = shape(w, h, f);
    let body_n = rows.len().saturating_sub(f.buttons + f.head);
    if f.squeeze && body_n as f32 * s.pitch > s.body[3] {
        s.pitch = (s.body[3] / body_n as f32).floor().max((s.pitch * 0.8).floor());
    }
    let (u, x0) = (s.u, s.x0);
    let p = menu::pal();
    let pad = 3.4 * u;
    let hair = (0.1 * u).max(1.0);

    // The panel itself: solid, with a lit edge on the side that faces the
    // world.
    hud.rect(x0, 0.0, BAND * u, h, with_a(p.plum, 1.0));
    match f.side {
        Side::Right => {
            hud.grad_h(x0 - 2.2 * u, 0.0, 2.2 * u, h, with_a(p.plum, 0.0), with_a(p.plum, 1.0));
            hud.rect(x0, 0.0, hair, h, with_a(p.apricot, 0.5));
        }
        Side::Left => {
            let x1 = x0 + BAND * u;
            hud.grad_h(x1, 0.0, 2.2 * u, h, with_a(p.plum, 1.0), with_a(p.plum, 0.0));
            hud.rect(x1 - hair, 0.0, hair, h, with_a(p.apricot, 0.5));
        }
    }

    m.at = m.at.min(rows.len().saturating_sub(1));
    m.hits.clear();
    m.minus.clear();
    m.plus.clear();
    m.drops.clear();
    for _ in rows {
        m.hits.push([0.0; 4]);
        m.minus.push([0.0; 4]);
        m.plus.push([0.0; 4]);
        m.drops.push([0.0; 4]);
    }

    // ---- head: what is being edited ----
    let ht = Type::new(Face::CapsBold, 2.5 * u, 0.1);
    let room = BAND * u - pad * 2.0;
    let wide = Hud::measure(ht, f.title);
    let ht = if wide > room && wide > 0.0 { Type::new(Face::CapsBold, 2.5 * u * (room / wide), 0.1) } else { ht };
    hud.set_lit(ht, x0 + pad, 4.6 * u, Ink::Grad(&p.lit), 0.8, f.title);

    let head = f.head.min(rows.len());
    for (i, r) in rows.iter().enumerate().take(head) {
        let ry = 6.0 * u + i as f32 * s.pitch;
        row(hud, m, &s, r, i, i == m.at, ry);
    }
    if head > 0 {
        hud.rect(x0 + pad, (6.0 * u + head as f32 * s.pitch + 0.8 * u).round(), room, hair, with_a(p.ink, 0.22));
    }

    // ---- body: scrolling ----
    let (lo, hi) = (head, rows.len().saturating_sub(f.buttons).max(head));
    let n = hi - lo;
    let fit = ((s.body[3] / s.pitch).floor() as usize).max(1);
    *shown = fit;
    *top = (*top).min(n.saturating_sub(fit));
    for k in 0..fit.min(n) {
        let i = lo + *top + k;
        if i >= hi {
            break;
        }
        let ry = s.body[1] + k as f32 * s.pitch;
        row(hud, m, &s, &rows[i], i, i == m.at, ry);
    }
    // The scrollbar, only when there is more than fits.
    m.track = [0.0; 4];
    m.thumb = [0.0; 4];
    if n > fit {
        let tx = x0 + BAND * u - 1.5 * u;
        let (ty, th) = (s.body[1], s.body[3]);
        hud.rect(tx, ty, 0.3 * u, th, with_a(p.ink, 0.12));
        let frac = fit as f32 / n as f32;
        let hgt = (th * frac).max(2.0 * u);
        let run = th - hgt;
        let at = *top as f32 / (n - fit) as f32;
        hud.rect(tx, (ty + run * at).round(), 0.3 * u, hgt, with_a(p.apricot, 0.55));
        m.track = [tx - 0.6 * u, ty, 1.5 * u, th];
        m.thumb = [tx - 0.6 * u, (ty + run * at).round(), 1.5 * u, hgt];
    }

    // ---- foot: what the picked row does, then the buttons ----
    let fy = s.body[1] + s.body[3] + 1.2 * u;
    hud.rect(x0 + pad, fy.round(), room, hair, with_a(p.ink, 0.22));
    let nt = Type::new(Face::Italic, 1.32 * u, 0.01);
    let help = rows.get(m.at).map(|r| r.item.help).unwrap_or("");
    for (i, line) in Hud::wrap(nt, help, room).iter().take(f.help_lines.max(1)).enumerate() {
        hud.set_lit(nt, x0 + pad, fy + 2.2 * u + i as f32 * 1.65 * u, Ink::Flat(p.note), 0.7, line);
    }

    let bw = (room - 2.0 * u) / 3.0;
    let bh = 3.2 * u;
    let by = h - 6.4 * u;
    for (k, i) in (hi..rows.len()).enumerate() {
        let bx = x0 + pad + k as f32 * (bw + 1.0 * u);
        button(hud, m, &rows[i], i, i == m.at, [bx, by, bw, bh], u);
    }
    let ft = Type::new(Face::Italic, 1.22 * u, 0.01);
    hud.set_lit(ft, x0 + pad, h - 1.9 * u, Ink::Flat(p.foot), 0.7, &menu::soften(f.foot));
}

/// One row of the form.
fn row(hud: &mut Hud, m: &mut Menu, s: &Shape, r: &Row, i: usize, lit: bool, ry: f32) {
    let (u, x0) = (s.u, s.x0);
    let p = menu::pal();
    let pad = 3.4 * u;
    let hair = (0.1 * u).max(1.0);
    let mid = ry + s.pitch * 0.5;
    let room = BAND * u - pad * 2.0;
    m.hits[i] = [x0 + 1.0 * u, ry, BAND * u - 2.5 * u, s.pitch];

    if let Field::Section { open } = r.field {
        // A heading: bolder, a triangle that says which way it goes, and a
        // rule across the panel so the eye can find the group it wants.
        let t = Type::new(Face::CapsBold, 1.3 * u, 0.14);
        if lit {
            hud.rect(x0 + 1.0 * u, ry, BAND * u - 2.5 * u, s.pitch, with_a(p.apricot, 0.11));
        }
        let c = if lit { p.apricot } else { with_a(p.ink, 0.85) };
        tri(hud, x0 + pad, mid - 0.5 * u, 1.1 * u, open, c);
        hud.set_lit(t, x0 + pad + 2.2 * u, mid + t.cap() * 0.5, Ink::Flat(c), 0.8, &r.item.label);
        hud.rect(x0 + pad, (ry + s.pitch - hair).round(), room, hair, with_a(p.ink, 0.14));
        return;
    }
    if lit {
        hud.rect(x0 + 1.0 * u, ry, BAND * u - 2.5 * u, s.pitch, with_a(p.apricot, 0.13));
        hud.rect(x0 + 1.0 * u, ry, 0.25 * u, s.pitch, with_a(p.apricot, 0.9));
    }

    // The label.
    let lx = x0 + pad + r.indent as f32 * 1.4 * u;
    let lt = Type::new(if lit { Face::CapsBold } else { Face::Caps }, 1.12 * u, 0.1);
    let ink = if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink) };
    hud.set_lit(lt, lx, mid + lt.cap() * 0.5, ink, if lit { 0.9 } else { 0.6 }, &r.item.label);

    // The field box, right-aligned in the panel.
    let bw = 17.0 * u;
    let bh = (s.pitch - 0.8 * u).max(1.6 * u);
    let bx = x0 + BAND * u - pad - bw - 1.2 * u;
    let by = (mid - bh * 0.5).round();
    let box_r = [bx, by, bw, bh];
    let vt = Type::new(Face::Italic, 1.34 * u, 0.01);
    let value = r.item.value.clone().unwrap_or_default();
    let vink = if r.dim {
        with_a(p.ink_value, 0.42)
    } else if lit {
        p.apricot
    } else {
        p.ink_value
    };
    let edge = |lit: bool| with_a(if lit { p.apricot } else { p.ink }, if lit { 0.8 } else { 0.28 });

    match r.field {
        Field::Plain | Field::Section { .. } | Field::Button => {}
        Field::Go(word) => {
            // A line that only does something: a narrow box on the end rather
            // than a field, so it does not read as something with a value.
            let gw = 6.0 * u;
            let gr = [bx + bw - gw, by, gw, bh];
            hud.rect(gr[0], gr[1], gr[2], gr[3], with_a(p.ink, if lit { 0.12 } else { 0.05 }));
            frame(hud, gr, hair, with_a(if lit { p.apricot } else { p.ink }, if lit { 0.8 } else { 0.22 }));
            let t = Type::new(Face::Caps, 1.04 * u, 0.08);
            let wide = Hud::measure(t, word);
            let c = if lit { p.apricot } else { with_a(p.ink, 0.6) };
            hud.set_lit(t, gr[0] + gw * 0.5 - wide * 0.5, mid + t.cap() * 0.5, Ink::Flat(c), 0.6, word);
        }
        Field::Flag(on) => {
            // A tick box, which is shorter to read than the word.
            let side = bh.min(1.9 * u);
            let cx = bx + bw - side;
            let cr = [cx, (mid - side * 0.5).round(), side, side];
            hud.rect(cr[0], cr[1], cr[2], cr[3], with_a(p.ink, if on == Some(true) { 0.0 } else { 0.06 }));
            if on == Some(true) {
                hud.rect(cr[0] + side * 0.22, cr[1] + side * 0.22, side * 0.56, side * 0.56, with_a(p.apricot, 0.95));
            }
            frame(hud, cr, hair, with_a(if lit { p.apricot } else { p.ink }, if lit { 0.8 } else { 0.3 }));
            if on.is_none() {
                let t = Type::new(Face::Italic, 1.2 * u, 0.01);
                hud.set_lit(t, cx - 3.4 * u, mid + t.cap() * 0.5, Ink::Flat(with_a(p.ink_value, 0.42)), 0.5, "unset");
            }
        }
        Field::Colour(c) => {
            hud.rect(box_r[0], box_r[1], box_r[2], box_r[3], [0.02, 0.02, 0.03, 0.85]);
            hud.rect(bx + hair, by + hair, 4.5 * u, bh - hair * 2.0, c);
            let wide = Hud::measure(vt, &value);
            hud.set_lit(vt, bx + bw - 0.9 * u - wide, mid + vt.cap() * 0.5, Ink::Flat(vink), 0.6, &value);
            frame(hud, box_r, hair, edge(lit));
            m.drops[i] = box_r;
        }
        Field::List => {
            hud.rect(box_r[0], box_r[1], box_r[2], box_r[3], with_a(p.ink, 0.055));
            frame(hud, box_r, hair, edge(lit));
            let cw = menu::caret_w(u);
            hud.set_lit(vt, bx + 0.9 * u, mid + vt.cap() * 0.5, Ink::Flat(vink), 0.6, &value);
            menu::caret(hud, bx + bw - cw - 0.8 * u, mid, u, with_a(if lit { p.apricot } else { p.ink }, if lit { 0.9 } else { 0.5 }));
            // The list hangs under the box.
            m.drops[i] = box_r;
        }
        Field::Step | Field::Stop(_) => {
            hud.rect(box_r[0], box_r[1], box_r[2], box_r[3], with_a(p.ink, 0.055));
            frame(hud, box_r, hair, edge(lit));
            // The value sits in the middle of what is left of the box, with a
            // stepper either side, each its own target.
            let aw = 2.2 * u;
            let at = Type::new(Face::Caps, 1.5 * u, 0.0);
            let ac = with_a(if lit { p.apricot } else { p.ink }, if lit { 0.95 } else { 0.35 });
            hud.set_lit(at, bx + 0.7 * u, mid + at.cap() * 0.5, Ink::Flat(ac), 0.6, "\u{2039}");
            let rw = Hud::measure(at, "\u{203a}");
            hud.set_lit(at, bx + bw - 0.7 * u - rw, mid + at.cap() * 0.5, Ink::Flat(ac), 0.6, "\u{203a}");
            let mut from = bx;
            if let Field::Stop(c) = r.field {
                // The stop's colour just inside the left arrow.
                let cw = 2.6 * u;
                let cx = bx + aw;
                hud.rect(cx, by + hair * 2.0, cw, bh - hair * 4.0, c);
                from = cx + cw;
            }
            let wide = Hud::measure(vt, &value);
            let centre = (from + bx + bw) * 0.5;
            hud.set_lit(vt, centre - wide * 0.5, mid + vt.cap() * 0.5, Ink::Flat(vink), 0.6, &value);
            m.minus[i] = [bx, by, aw, bh];
            m.plus[i] = [bx + bw - aw, by, aw, bh];
        }
    }
    if let Some(note) = &r.item.note {
        let t = Type::new(Face::Italic, 1.1 * u, 0.01);
        hud.set_lit(t, bx - 1.0 * u - Hud::measure(t, note), mid + t.cap() * 0.5, Ink::Flat(with_a(p.apricot, 0.8)), 0.5, note);
    }
}

/// One of the buttons pinned at the foot.
fn button(hud: &mut Hud, m: &mut Menu, r: &Row, i: usize, lit: bool, b: [f32; 4], u: f32) {
    let p = menu::pal();
    let hair = (0.1 * u).max(1.0);
    let keen = r.item.note.is_some();
    hud.rect(b[0], b[1], b[2], b[3], with_a(if keen { p.apricot } else { p.ink }, if lit { 0.22 } else { 0.07 }));
    frame(hud, b, hair, with_a(if lit || keen { p.apricot } else { p.ink }, if lit { 0.9 } else { 0.35 }));
    let t = Type::new(if lit { Face::CapsBold } else { Face::Caps }, 1.16 * u, 0.1);
    let wide = Hud::measure(t, &r.item.label);
    let ink = if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink) };
    hud.set_lit(t, b[0] + b[2] * 0.5 - wide * 0.5, b[1] + b[3] * 0.5 + t.cap() * 0.5, ink, 0.8, &r.item.label);
    m.hits[i] = b;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_steps_in_range_and_rounds_to_what_it_shows() {
        let n = Num::new(0.0, 1.0, 0.05, 2);
        assert_eq!(n.nudge(0.95, 1), 1.0);
        assert_eq!(n.nudge(1.0, 1), 1.0, "it went past the top");
        assert_eq!(n.nudge(0.0, -1), 0.0, "it went under the bottom");
        assert_eq!(n.nudge(0.42, 1), 0.47, "it did not land on two places");
        assert_eq!(n.text(0.5), "0.50");
        assert_eq!(Num::new(0.0, 900.0, 10.0, 0).reads(Unit::Metres).text(120.0), "120 m");
        let count = Num::new(0.0, 8.0, 1.0, 0).reads(Unit::Count);
        assert_eq!((count.text(0.0), count.text(3.0)), ("off".to_string(), "3".to_string()));
    }
}
