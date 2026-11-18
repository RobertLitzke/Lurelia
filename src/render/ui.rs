//! The standard in-game readouts: meters, numbers and captions, stacked in a
//! corner of the screen.
//!
//! `hud.rs` is the drawing layer — rectangles, text, gradients, at pixel
//! coordinates. Everything built on it used to work out its own corners, and
//! two readouts put in the same corner by different files overlapped (the
//! shooter's ammunition count sat on top of its weapon's name). This file is
//! the layer between: a `Corner` hands out one row at a time, each widget
//! says how tall it is, and nothing has to know what is above it.
//!
//! Anything that shows the player a number or a fraction belongs here: the
//! shooter's health, armour, ammunition and reload, the bike's boost, the
//! glider's charge.

// A widget set is a set: the corners, tints and measures that no readout
// happens to want today are still the vocabulary the next one is written
// in, and dropping them would only mean the next caller writes its own
// arithmetic again, which is the thing this file is here to stop.
#![allow(dead_code)]

use super::hud::Hud;

/// Which corner a stack grows from, and which way its rows run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    /// Centred across the screen, growing upward from the bottom.
    BottomMiddle,
}

impl Side {
    fn right(self) -> bool {
        matches!(self, Side::TopRight | Side::BottomRight)
    }
    fn middle(self) -> bool {
        self == Side::BottomMiddle
    }
    /// Rows run upward from the bottom edge.
    fn up(self) -> bool {
        matches!(self, Side::BottomLeft | Side::BottomRight | Side::BottomMiddle)
    }
}

/// The colours a widget is drawn in. `warn` replaces `fill` once a meter
/// falls under `low`.
#[derive(Clone, Copy, Debug)]
pub struct Tint {
    pub fill: [f32; 4],
    pub warn: [f32; 4],
    pub low: f32,
    pub ink: [f32; 4],
}

impl Default for Tint {
    fn default() -> Self {
        Tint {
            fill: [0.85, 0.9, 0.9, 0.85],
            warn: [0.9, 0.25, 0.2, 0.9],
            low: 0.0,
            ink: [1.0, 0.97, 0.94, 0.9],
        }
    }
}

impl Tint {
    pub fn solid(fill: [f32; 4]) -> Tint {
        Tint {
            fill,
            warn: fill,
            ..Default::default()
        }
    }
    /// A meter that turns `warn` under `low` of its span.
    pub fn warning(fill: [f32; 4], warn: [f32; 4], low: f32) -> Tint {
        Tint {
            fill,
            warn,
            low,
            ..Default::default()
        }
    }
    fn of(&self, frac: f32) -> [f32; 4] {
        if frac < self.low {
            self.warn
        } else {
            self.fill
        }
    }
}

/// A column of readouts in one corner. Rows are taken in order and never
/// overlap: each widget asks for the height it needs, and the next starts
/// after it.
pub struct Corner {
    side: Side,
    /// The scale everything is drawn at: `s` in the HUD's own units.
    pub s: f32,
    /// The edge the rows are aligned to, and the one they grow from.
    edge: f32,
    cursor: f32,
    /// How wide a full-span meter is.
    pub span: f32,
}

/// A row's rectangle: where a widget may draw.
pub struct Row {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Corner {
    /// A stack in `side` of a `w` by `h` screen, at scale `s`.
    pub fn new(side: Side, w: f32, h: f32, s: f32) -> Corner {
        let pad = 16.0 * s;
        let edge = match side {
            Side::TopLeft | Side::BottomLeft => pad,
            Side::TopRight | Side::BottomRight => w - pad,
            Side::BottomMiddle => (w * 0.5).floor(),
        };
        Corner {
            side,
            s,
            edge,
            cursor: if side.up() { h - pad } else { pad },
            span: 120.0 * s,
        }
    }

    /// The pixel pitch of one line of ordinary text.
    pub fn line(&self) -> f32 {
        10.0 * self.s
    }

    /// Nothing, `n` lines tall: a deliberate space between blocks.
    pub fn gap(&mut self, lines: f32) {
        self.step(self.line() * lines);
    }

    /// Take a row `h` tall and say where it is.
    pub fn row(&mut self, w: f32, h: f32) -> Row {
        let x = if self.side.right() {
            self.edge - w
        } else if self.side.middle() {
            self.edge - w * 0.5
        } else {
            self.edge
        };
        let y = if self.side.up() { self.cursor - h } else { self.cursor };
        self.step(h);
        Row { x: x.floor(), y: y.floor(), w, h }
    }

    fn step(&mut self, h: f32) {
        if self.side.up() {
            self.cursor -= h;
        } else {
            self.cursor += h;
        }
    }

    /// A line of text in the stack, aligned to its edge. `scale` multiplies
    /// the stack's own.
    pub fn text(&mut self, hud: &mut Hud, scale: f32, ink: [f32; 4], text: &str) {
        let s = self.s * scale;
        let r = self.row(Hud::text_width(text, s), 10.0 * s);
        hud.label(self.place(&r, r.w), r.y, s, ink, text);
    }

    /// A meter: a dark trough with `frac` of it filled. `thick` is its
    /// height in lines of text (0.6 is the usual hairline bar).
    pub fn meter(&mut self, hud: &mut Hud, frac: f32, thick: f32, t: Tint) {
        let frac = frac.clamp(0.0, 1.0);
        let h = (self.line() * thick).max(2.0);
        let r = self.row(self.span, h + 2.0 * self.s);
        let (x, y) = (self.place(&r, self.span), r.y + self.s);
        hud.rect(x - 1.0, y - 1.0, r.w + 2.0, h + 2.0, [0.0, 0.0, 0.0, 0.45]);
        hud.rect(x, y, r.w * frac, h, t.of(frac));
    }

    /// Where a part `w` wide sits inside a row, on the stack's own edge.
    fn place(&self, r: &Row, w: f32) -> f32 {
        if self.side.right() {
            r.x + r.w - w
        } else if self.side.middle() {
            r.x + (r.w - w) * 0.5
        } else {
            r.x
        }
        .floor()
    }

    /// A number with a word under it: the way every count in the game is
    /// shown. The number is drawn at 2.4 times the stack's scale and the
    /// caption at 1.0.
    pub fn readout(&mut self, hud: &mut Hud, number: &str, caption: &str, t: Tint) {
        self.block(hud, number, caption, None, t);
    }

    /// A number, a bar under it and a word under that: health, armour,
    /// ammunition, a reload.
    pub fn gauge(&mut self, hud: &mut Hud, number: &str, caption: &str, frac: f32, t: Tint) {
        self.block(hud, number, caption, Some(frac), t);
    }

    /// The one place a widget of several parts is laid out. It takes a
    /// single row as tall as all of its parts and places them inside it,
    /// rather than taking a row each: a stack that grows upward hands out
    /// its rows from the bottom, so three rows taken in reading order come
    /// out upside down. One row, read downward, is the same block whichever
    /// corner it is in, and it is why nothing here can overlap its own
    /// caption -- which is what the shooter's ammunition count used to do to
    /// its weapon's name.
    fn block(&mut self, hud: &mut Hud, number: &str, caption: &str, bar: Option<f32>, t: Tint) {
        let ns = self.s * 2.4;
        let num_h = if number.is_empty() { 0.0 } else { 11.0 * ns };
        let bar_h = bar.map_or(0.0, |_| (self.line() * 0.6).max(2.0) + 3.0 * self.s);
        let cap_h = if caption.is_empty() { 0.0 } else { 10.0 * self.s };
        let span = if bar.is_some() { self.span } else { 0.0 };
        let width = span.max(Hud::text_width(number, ns)).max(Hud::text_width(caption, self.s));
        let r = self.row(width, num_h + bar_h + cap_h);
        let mut y = r.y;
        if !number.is_empty() {
            hud.label(self.place(&r, Hud::text_width(number, ns)), y, ns, t.ink, number);
            y += num_h;
        }
        if let Some(frac) = bar {
            let frac = frac.clamp(0.0, 1.0);
            let x = self.place(&r, self.span);
            let hi = (self.line() * 0.6).max(2.0);
            hud.rect(x - 1.0, y - 1.0, self.span + 2.0, hi + 2.0, [0.0, 0.0, 0.0, 0.45]);
            hud.rect(x, y, self.span * frac, hi, t.of(frac));
            y += bar_h;
        }
        if !caption.is_empty() {
            let mut ink = t.ink;
            ink[3] *= 0.6;
            hud.label(self.place(&r, Hud::text_width(caption, self.s)), y, self.s, ink, caption);
        }
    }

    /// How far down (or up) the stack has reached: what a widget outside it
    /// has to clear. Always a pixel coordinate on the screen.
    pub fn reached(&self) -> f32 {
        self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows in one stack never overlap, whichever corner it grows from.
    #[test]
    fn a_stack_does_not_overlap_itself() {
        for side in [Side::TopLeft, Side::TopRight, Side::BottomLeft, Side::BottomRight, Side::BottomMiddle] {
            let mut c = Corner::new(side, 1920.0, 1080.0, 2.0);
            let rows: Vec<(f32, f32)> = (0..5).map(|i| c.row(100.0 + i as f32 * 10.0, 20.0)).map(|r| (r.y, r.h)).collect();
            for (i, a) in rows.iter().enumerate() {
                for b in &rows[i + 1..] {
                    let overlap = (a.0 + a.1).min(b.0 + b.1) - a.0.max(b.0);
                    assert!(overlap <= 0.0, "{side:?}: rows {a:?} and {b:?} overlap by {overlap}");
                }
            }
        }
    }

    /// A gauge takes its number, bar and caption out of the stack in one
    /// block, so the next widget starts clear of all three.
    #[test]
    fn a_gauge_claims_its_whole_block() {
        let mut hud = Hud::default();
        let mut c = Corner::new(Side::BottomLeft, 1920.0, 1080.0, 2.0);
        let before = c.reached();
        c.gauge(&mut hud, "100", "HEALTH", 1.0, Tint::default());
        let used = before - c.reached();
        // The number alone is 11 lines of its own scale: the block is taller.
        assert!(used > 11.0 * 2.0 * 2.4, "a gauge took only {used} px");
        let after = c.reached();
        let r = c.row(10.0, 10.0);
        assert!(r.y + r.h <= after + 0.01, "the next row ran into the gauge");
    }

    /// A block reads downward -- number, bar, caption -- whether the stack
    /// grows up from the bottom of the screen or down from the top. This is
    /// the bug the file exists to make impossible: parts taken as separate
    /// rows come out in reverse in an upward stack.
    #[test]
    fn a_block_reads_downward_either_way() {
        for side in [Side::BottomLeft, Side::TopLeft] {
            let mut hud = Hud::default();
            let mut c = Corner::new(side, 1920.0, 1080.0, 2.0);
            c.gauge(&mut hud, "100", "HEALTH", 0.5, Tint::default());
            // The glyphs of the number must all sit above those of the
            // caption, and the bar between them.
            let ys: Vec<f32> = hud.items.iter().map(|i| i.rect[1]).collect();
            let (first, last) = (ys.first().copied().unwrap(), ys.last().copied().unwrap());
            assert!(first < last, "{side:?}: the number ({first}) is not above the caption ({last})");
        }
    }

    /// A right-hand stack puts its rows' right edges on one line.
    #[test]
    fn a_right_stack_is_right_aligned() {
        let mut c = Corner::new(Side::BottomRight, 1000.0, 800.0, 2.0);
        let a = c.row(120.0, 10.0);
        let b = c.row(40.0, 10.0);
        assert_eq!(a.x + a.w, b.x + b.w);
    }
}
