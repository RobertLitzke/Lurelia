//! The EDIT page drawn as a form down the right of the screen (src/form.rs),
//! the entry's head pinned at the top and its buttons at the foot.

use crate::form::{self, Form, Row, Side};
use crate::menu::Menu;
use crate::render::hud::Hud;

/// Draw the panel. `rows` is `Editor::rows()`; `title` and `foot` the lines
/// over and under it.
#[allow(clippy::too_many_arguments)]
pub fn draw(hud: &mut Hud, m: &mut Menu, page: &mut super::Editor, rows: &[Row], title: &str, foot: &str, w: f32, h: f32) {
    let f = Form {
        title,
        foot,
        head: page.head_rows().len(),
        buttons: page.foot_rows().len(),
        side: Side::Right,
        help_lines: 2,
        squeeze: false,
    };
    let (mut top, mut shown) = (page.top, page.shown);
    form::draw(hud, m, rows, &f, &mut top, &mut shown, w, h);
    (page.top, page.shown) = (top, shown);
}
