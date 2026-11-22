use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::places::Sign;
use glam::Vec3;
use serde::Deserialize;

/// A sign: a board with a line or two on it, and whatever holds it up. The
/// style (`library/signs/`) decides the board's size and colours, the
/// lettering and the structure placed under it.
#[derive(Deserialize, Debug)]
pub struct SignEvent {
    at: Pos,
    /// One string a line; two or three short lines read best.
    text: Vec<String>,
    /// The style's name in `library/signs/`.
    #[serde(default = "default_style")]
    style: String,
    /// The compass bearing the front of the board faces (the direction a
    /// reader stands in). Quarter turns for the post it stands on.
    #[serde(default)]
    bearing: f32,
    /// Face toward this place instead of a bearing.
    #[serde(default)]
    face: Option<Pos>,
    /// Slot overrides for the post structure.
    #[serde(default)]
    slots: HashMap<String, String>,
    /// Years of neglect, for the post's decay.
    #[serde(default)]
    age: f32,
    /// Put the board's centre this far above the ground instead of the
    /// style's own height.
    #[serde(default)]
    centre_m: Option<f32>,
}

fn default_style() -> String {
    "timber_sign".to_string()
}

impl Process for SignEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let SignEvent { at, text, style, bearing, face, slots, age, centre_m } = self;
        let asked = cx.pos(at);
        let Some(si) = cx.lib.sign_style(style) else {
            cx.warnings.push(format!("sign: unknown style {style}"));
            return "skipped".into();
        };
        let sd = cx.lib.signs[si].clone();
        let face_at = face.as_ref().map(|t| cx.pos(t));
        let b0 = match face_at {
            Some(t) => crate::util::bearing(t - asked).to_degrees(),
            None => *bearing,
        };
        // A sign put down by offset from a place can land on a building the
        // village or town put there first: step it out to the nearest clear
        // ground, trying the side it faces first. A board with no post of its
        // own is hung on something, so it stays where it was put.
        let p = if sd.post.is_some() { clear_spot(cx, asked, sd.board[0] * 0.5 + 1.0, b0) } else { asked };
        let b = match face_at {
            Some(t) => crate::util::bearing(t - p).to_degrees(),
            None => *bearing,
        };
        // The post first: it levels the ground, so the board's height is read
        // from the ground afterwards. Its voxels land on the quarter-metre
        // grid, which can move its face by more than the board's clearance,
        // so the board is hung off the face the structure actually came out
        // with rather than off the point the event asked for.
        let mut front = p;
        // The board hangs at `centre_m` above the foot of whatever holds it
        // up. Post structures settle into the ground (`level: false` takes
        // the lowest ground under the footprint, a quarter-metre down), so
        // measuring from the ground under the sign's own point instead left
        // the board floating clear of its posts on any slope.
        let mut foot = cx.f.height(p.x, p.y);
        if let Some(post) = &sd.post {
            match cx.lib.structure(post) {
                // The post faces the reader, so its front is the board's front.
                Some(d) => {
                    if let Some(k) = cx.place(d, p, b, slots, *age, cx.seed) {
                        let (x0, z0, x1, z1) = cx.placed[k].footprint(&cx.shapes);
                        let c = glam::Vec2::new((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125);
                        let half = glam::Vec2::new((x1 - x0) as f32 * 0.125, (z1 - z0) as f32 * 0.125);
                        let n = glam::Vec2::new(b.to_radians().sin(), -b.to_radians().cos());
                        front = c + n * (n.x.abs() * half.x + n.y.abs() * half.y);
                        foot = cx.placed[k].origin[1] as f32 * 0.25;
                    }
                }
                None => cx.warnings.push(format!("sign {style}: unknown structure {post}")),
            }
        }
        // Too many words on too small a board: the letters shrink to fit, and
        // past a point nobody can read them.
        let lines: Vec<&str> = text.iter().map(|s| s.as_str()).collect();
        let cap = crate::world::places::fit_cap(&sd, &lines);
        if cap < 0.055 || (sd.cap_m > 0.0 && cap < sd.cap_m * 0.62) {
            cx.warnings.push(format!(
                "sign \"{}\" ({style}): the text comes out {:.0} mm tall on a {:.1} x {:.1} m board; use fewer or shorter lines",
                text.join(" / "),
                cap * 1000.0,
                sd.board[0],
                sd.board[1]
            ));
        }
        let y = foot + centre_m.unwrap_or(sd.centre_m);
        // The board stands clear of whatever holds it up.
        let r = b.to_radians();
        let board = front + glam::Vec2::new(r.sin(), -r.cos()) * (sd.thickness * 0.5 + sd.offset_m);
        cx.signs.push(Sign { at: Vec3::new(board.x, y, board.y), bearing: b, style: si, text: text.clone() });
        let moved = if p.distance(asked) > 0.1 { format!(" (moved {:.0} m off a building)", p.distance(asked)) } else { String::new() };
        format!("sign \"{}\" at {:.0},{:.0} facing {b:.0} deg{moved}", text.join(" / "), p.x, p.y)
    }
}

/// Distance from `p` to the nearest structure footprint already placed, in
/// metres (0 inside one).
fn gap_to_structures(cx: &EventCtx, p: glam::Vec2) -> f32 {
    let mut best = f32::MAX;
    for s in &cx.placed {
        let (x0, z0, x1, z1) = s.footprint(&cx.shapes);
        let (x0, z0, x1, z1) = (x0 as f32 * 0.25, z0 as f32 * 0.25, x1 as f32 * 0.25, z1 as f32 * 0.25);
        let dx = (x0 - p.x).max(p.x - x1).max(0.0);
        let dz = (z0 - p.y).max(p.y - z1).max(0.0);
        best = best.min((dx * dx + dz * dz).sqrt());
    }
    best
}

/// `p` if nothing placed comes within `r` of it, else the nearest point (in
/// one-metre rings out to 40 m, sixteen headings a ring starting at
/// `bearing`) that is clear. Falls back to `p` when nothing is.
fn clear_spot(cx: &EventCtx, p: glam::Vec2, r: f32, bearing: f32) -> glam::Vec2 {
    if gap_to_structures(cx, p) >= r {
        return p;
    }
    for d in 1..=40 {
        for k in 0..16 {
            // Alternate either side of the bearing: 0, +1, -1, +2, ...
            let step = if k % 2 == 1 { (k + 1) / 2 } else { -(k / 2) } as f32;
            let a = (bearing + step * 22.5).to_radians();
            let q = p + glam::Vec2::new(a.sin(), -a.cos()) * d as f32;
            if gap_to_structures(cx, q) >= r {
                return q;
            }
        }
    }
    p
}
