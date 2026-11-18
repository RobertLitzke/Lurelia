//! The page as a form: one scrolling list of labelled fields under headings,
//! in place of the stack of menus it used to be.
//!
//! The old page was a menu: a front page with the groups on it, and a page of
//! its own for each group that you walked into and backed out of. That is the
//! wrong shape for an editor -- you cannot see the entry, only one group of
//! it, and every look at another group costs two keypresses. Here the whole
//! entry is one scroll: a pinned head saying what is being edited, headings
//! that open and close, a row to a field, and the buttons pinned at the foot.
//!
//! A row carries the `menu::Item` the rest of the page already works in, so
//! clicking and stepping go on working exactly as they did, plus what the
//! panel needs to draw it as a form: which kind of field it is, how deep it
//! sits, and whether the value is the entry's own.

use super::{get, knobs, pages, shown_name, Editor, Kind, Val};
use crate::form::{Field, Row};
use crate::menu::{Choice, Item};

/// sRGB 0..1 to the linear the HUD paints in, for a colour row's swatch.
fn lin4(c: [f32; 3]) -> [f32; 4] {
    let f = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2]), 1.0]
}

/// The headings, in order. Section 0 is what is being shown rather than what
/// the entry says; the rest are the entry's own groups.
pub const SHOWING: &str = "SHOWING";

pub fn sections(kind: Kind) -> Vec<&'static str> {
    let mut out = vec![SHOWING];
    out.extend(pages(kind));
    out
}

impl Editor {
    /// Whether heading `i` is open.
    pub fn section_open(&self, i: usize) -> bool {
        self.closed & (1 << i.min(31)) == 0
    }

    pub fn toggle_section(&mut self, i: usize) {
        self.closed ^= 1 << i.min(31);
    }

    /// Open one heading and close the rest (`--edit NAME:3`).
    pub fn only_section(&mut self, i: usize) {
        self.closed = !0u32 & !(1 << i.min(31));
    }

    /// The two rows pinned at the head of the panel: what is being edited.
    pub fn head_rows(&self) -> Vec<Row> {
        let mut out = vec![
            Row::new(Item::drop("KIND", Choice::EditKind, self.kind.label().to_string()), Field::List),
            Row::new(
                Item::drop("ENTRY", Choice::EditEntry, format!("{} ({}/{})", shown_name(self.name()), self.at + 1, self.names.len())),
                Field::List,
            ),
        ];
        for r in out.iter_mut() {
            r.item.help = super::front_help(r.item.choice);
        }
        out
    }

    /// The three rows pinned at the foot, drawn as buttons.
    pub fn foot_rows(&self) -> Vec<Row> {
        let mut save = Item::new("WRITE", Choice::EditExport);
        if self.dirty {
            save.note = Some("edited".into());
        }
        let mut out = vec![
            Row::new(save, Field::Button),
            Row::new(Item::new("REREAD", Choice::EditReload), Field::Button),
            Row::new(Item::new("CLOSE", Choice::Back), Field::Button),
        ];
        for r in out.iter_mut() {
            r.item.help = super::front_help(r.item.choice);
        }
        out
    }

    /// The scrolling body: every heading, with its fields under it when it is
    /// open.
    pub fn body_rows(&self) -> Vec<Row> {
        let mut out: Vec<Row> = Vec::new();
        let full = self.resolved();
        let mine = &self.doc;
        for (i, name) in sections(self.kind).iter().enumerate() {
            let open = self.section_open(i);
            out.push(Row::new(Item::new(name, Choice::EditPage(i as u8)), Field::Section { open }));
            if !open {
                continue;
            }
            if i == 0 {
                out.extend(self.showing_rows());
                continue;
            }
            let page = i as u8;
            let colours = self.colour_rows(page);
            if pages(self.kind).get(i - 1) == Some(&"COLOURS") {
                for (k, (label, path)) in colours.iter().enumerate() {
                    let c = self.colour(page, k);
                    let mut it = Item::value(label, Choice::EditColour(page, k as u8), super::hexed(c));
                    it.swatch = Some(c);
                    it.help = "One of the entry's colour slots. Open it to paint it; the model is rebuilt as you go.";
                    out.push(Row::new(it, Field::Colour(lin4(c))).under().dimmed(get(mine, path).is_none()));
                }
                if colours.is_empty() {
                    out.push(Row::new(Item::new("(no colours)", Choice::EditPage(i as u8)), Field::Plain).under());
                }
                continue;
            }
            for (k, knob) in knobs(self.kind).iter().enumerate() {
                if knob.page != page {
                    continue;
                }
                let value = self.shown(k);
                let own = get(mine, knob.reach.path).is_some();
                let row = match knob.val {
                    Val::Pick(_) | Val::Mat => Row::new(Item::drop(knob.label, Choice::EditKnob(k as u8), value), Field::List),
                    Val::Flag => {
                        let on = get(&full, knob.reach.path).and_then(|v| v.as_bool());
                        Row::new(Item::value(knob.label, Choice::EditKnob(k as u8), value), Field::Flag(on))
                    }
                    Val::Colour => {
                        let at = colours.iter().position(|(_, path)| path == knob.reach.path).unwrap_or(0);
                        let c = self.colour(page, at);
                        let mut it = Item::value(knob.label, Choice::EditColour(page, at as u8), value);
                        it.swatch = Some(c);
                        Row::new(it, Field::Colour(lin4(c)))
                    }
                    Val::Num(_) => Row::new(Item::value(knob.label, Choice::EditKnob(k as u8), value), Field::Step),
                };
                let mut row = row.under().dimmed(!own);
                row.item.help = knob.help;
                out.push(row);
            }
        }
        out
    }

    /// Section 0: how the subject is being shown, which is the page's own
    /// state rather than anything in the file.
    fn showing_rows(&self) -> Vec<Row> {
        let mut out = Vec::new();
        if self.kind == Kind::Creature {
            out.push(Row::new(Item::value("AGE", Choice::EditAge, self.stage.name().to_string()), Field::Step));
            out.push(Row::new(Item::drop("POSE", Choice::EditPose, shown_name(self.pose.name())), Field::List));
        }
        out.push(Row::new(
            Item::value("GROUND", Choice::EditGrid, if self.grid { "Grey grid" } else { "The world" }.to_string()),
            Field::Step,
        ));
        // Only a creature and a plant are rolled: a gun and a found thing are
        // the same every time.
        if matches!(self.kind, Kind::Creature | Kind::Plant) {
            out.push(Row::new(Item::new("REGENERATE", Choice::EditRegen), Field::Go("ROLL")));
        }
        for r in out.iter_mut() {
            r.item.help = super::front_help(r.item.choice);
            r.indent = 1;
        }
        out
    }

    /// Every row of the panel in the order the keyboard walks them: the head,
    /// then the body, then the buttons.
    pub fn rows(&self) -> Vec<Row> {
        let mut out = self.head_rows();
        out.extend(self.body_rows());
        out.extend(self.foot_rows());
        out
    }

    /// Where the body starts and ends in `rows()`, so the panel knows which
    /// rows scroll and which are pinned.
    pub fn body_range(&self) -> (usize, usize) {
        let head = self.head_rows().len();
        (head, head + self.body_rows().len())
    }
}
