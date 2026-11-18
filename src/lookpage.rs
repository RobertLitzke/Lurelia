//! The LOOK page: the light, the distance ramp and the grade, all editable
//! while the world is on screen, with a colour picker for the gradient stops
//! and an EXPORT that writes the result back to `library/looks/<style>.json`.
//!
//! The page is drawn down the left so the world stays visible on the right,
//! and every change goes straight into the renderer's live Look and Grade
//! (render/grade.rs), so the frame behind it answers at once.

use crate::form::{self, Field, Form, Num, Row, Side, Unit, Val};
use crate::menu::{self, Choice, Item};
use crate::render::font::Face;
use crate::render::grade::{Grade, ROW_MAP, ROW_RAMP, ROW_TINT};
use crate::render::hud::{with_a, Hud, Ink, Type};
use crate::render::{Look, Style};
use glam::Vec3;
use serde::Serialize;

/// Swatch codes: the light's own colours, then a stop on each strip.
pub const SW_ZENITH: u8 = 0;
pub const SW_HORIZON: u8 = 1;
pub const SW_GROUND: u8 = 2;
pub const SW_AMBIENT: u8 = 3;
pub const SW_SUN: u8 = 4;
/// Stop `i` of a strip is its base plus i.
pub const SW_RAMP: u8 = 16;
pub const SW_TINT: u8 = 32;
pub const SW_MAP: u8 = 48;

/// The strip and stop a swatch code names, if it is a stop.
fn stop_of(code: u8) -> Option<(usize, usize)> {
    if code >= SW_MAP {
        Some((ROW_MAP, (code - SW_MAP) as usize))
    } else if code >= SW_TINT {
        Some((ROW_TINT, (code - SW_TINT) as usize))
    } else if code >= SW_RAMP {
        Some((ROW_RAMP, (code - SW_RAMP) as usize))
    } else {
        None
    }
}

/// The base code of the strip a row belongs to.
fn base_of(row: usize) -> u8 {
    match row {
        ROW_MAP => SW_MAP,
        ROW_TINT => SW_TINT,
        _ => SW_RAMP,
    }
}

/// How a LOOK knob reaches its value: a pair of functions on the look and
/// the grade.
#[derive(Clone, Copy)]
pub struct Access {
    pub get: fn(&Look, &Grade) -> f32,
    pub set: fn(&mut Look, &mut Grade, f32),
}

/// One number on the LOOK page (`form::Knob`, the same kind of line the EDIT
/// page has). `page` is the group it is shown in.
pub type KnobDef = form::Knob<Access>;

/// `fmt` is how the value reads: 0 two decimals, 1 whole metres, 2 a count
/// that reads "off" at zero.
#[allow(clippy::too_many_arguments)]
const fn knob(page: u8, label: &'static str, lo: f32, hi: f32, step: f32, fmt: u8, help: &'static str, get: fn(&Look, &Grade) -> f32, set: fn(&mut Look, &mut Grade, f32)) -> KnobDef {
    let num = match fmt {
        1 => Num::new(lo, hi, step, 0).reads(Unit::Metres),
        2 => Num::new(lo, hi, step, 0).reads(Unit::Count),
        _ => Num::new(lo, hi, step, 2),
    };
    KnobDef {
        page,
        label,
        help,
        val: Val::Num(num),
        reach: Access { get, set },
    }
}

/// The knob's number spec. Every LOOK knob is a number.
fn num_of(k: &KnobDef) -> Num {
    match k.val {
        Val::Num(n) => n,
        _ => Num::new(0.0, 1.0, 0.05, 2),
    }
}

pub const KNOBS: &[KnobDef] = &[
    // 0..4: the light.
    knob(
        1,
"SUN GAIN",
        0.2,
        2.5,
        0.02,
        0,
        "How bright the key light is. Everything else in the frame is judged against it, so a small change here moves the whole exposure.",
        |l, _| l.sun_gain,
        |l, _, v| l.sun_gain = v,
    ),
    knob(
        1,
"FOG DISTANCE",
        200.0,
        4000.0,
        50.0,
        1,
        "How far you can see before the air has taken over the colour. Lower is hazier. This is the overall thickness; HAZE DENSITY on the ramp page scales it again.",
        |l, _| 1.0 / l.fog_density.max(1e-6),
        |l, _, v| l.fog_density = 1.0 / v.max(1.0),
    ),
    knob(
        1,
"SUN-SIDE WARMTH",
        0.0,
        0.8,
        0.02,
        0,
        "Haze warmer looking into the sun and cooler looking away from it, which separates a sunlit slope from a shaded one at the same distance.",
        |_, g| g.sun_side,
        |_, g, v| g.sun_side = v,
    ),
    knob(
        1,
"SUN GLOW",
        0.0,
        3.0,
        0.1,
        0,
        "A broad warm bloom around the sun, in the sky and in the haze. Off by default; it is not the sun rays, which are their own pass.",
        |_, g| g.sun_glow,
        |_, g, v| g.sun_glow = v,
    ),
    knob(
        1,
"TIME OF DAY TINT",
        0.0,
        2.0,
        0.05,
        0,
        "How much of the clock's own warm and cool shift reaches the grade: gold through golden hour, blue-grey by moonlight. 0 holds one colour all day.",
        |_, g| g.time_tint,
        |_, g, v| g.time_tint = v,
    ),
    // 5..14: the distance ramp.
    knob(
        2,
"RAMP REACH",
        40.0,
        4000.0,
        20.0,
        1,
        "The distance the whole strip is spread over. Raise it for an open landscape; every stop's metres move with it, since a stop is a position along the strip.",
        |_, g| g.ramp_reach,
        |_, g, v| g.ramp_reach = v,
    ),
    knob(
        2,
"RAMP CURVE",
        0.0,
        1.0,
        0.05,
        0,
        "1 eases, spending the strip on the first few hundred metres and crowding everything beyond into its far end. 0 is straight, so a stop sits at the distance it says.",
        |_, g| g.ramp_curve,
        |_, g, v| g.ramp_curve = v,
    ),
    knob(
        2,
"DISTANCE SLABS",
        0.0,
        16.0,
        1.0,
        2,
        "Cut the view into this many slabs, each taking one colour and one thickness of haze right across it. This is what turns ridges into flat cards.",
        |_, g| g.ramp_bands,
        |_, g, v| g.ramp_bands = v,
    ),
    knob(
        2,
"HELD TO SKY VALUE",
        0.0,
        1.0,
        0.05,
        0,
        "At 1 only each stop's hue is used and its brightness follows the horizon. Drop it toward 0 and the strip sets the brightness too, which is what separates one band from the next.",
        |_, g| g.ramp_value,
        |_, g, v| g.ramp_value = v,
    ),
    knob(
        2,
"HAZE FROM RAMP",
        0.0,
        1.0,
        0.05,
        0,
        "How much of the haze colour comes from the strip rather than from the sky behind it. At 0 the ramp does nothing to the haze.",
        |_, g| g.haze_ramp,
        |_, g, v| g.haze_ramp = v,
    ),
    knob(
        2,
"HAZE DENSITY",
        0.1,
        5.0,
        0.05,
        0,
        "Thickness of the ramped haze, on top of FOG DISTANCE. The ramp only shows where the haze has taken over, so this decides how much of the strip you ever see.",
        |_, g| g.haze_density,
        |_, g, v| g.haze_density = v,
    ),
    knob(
        2,
"HAZE HEIGHT",
        0.0,
        600.0,
        10.0,
        1,
        "The height the haze thins over. A valley floor then lies under more air than the ridge behind it, which is what stacks the distance into layers instead of one flat wash. 0 gives air of one thickness at every height.",
        |_, g| g.haze_height,
        |_, g, v| g.haze_height = v,
    ),
    knob(
        2,
"SKY FROM RAMP",
        0.0,
        1.0,
        0.05,
        0,
        "Pull the far end of the strip into the low sky, so distant ridges dissolve into it instead of standing against it. Raise SKY REACH with it, or the band sits too low to see.",
        |_, g| g.sky_ramp,
        |_, g, v| g.sky_ramp = v,
    ),
    knob(
        2,
"SKY REACH",
        0.0,
        0.8,
        0.02,
        0,
        "How far up the sky that colour climbs. At 0.26 it is a thin band just above the horizon, which terrain usually hides: raise it to see SKY FROM RAMP at all.",
        |_, g| g.sky_reach,
        |_, g, v| g.sky_reach = v,
    ),
    knob(
        2,
"SHADE FROM RAMP",
        0.0,
        1.0,
        0.05,
        0,
        "Tint shadowed and indirect light by the strip at that distance, so a far hillside's shadows sit in the same family as the haze in front of it.",
        |_, g| g.shade_ramp,
        |_, g, v| g.shade_ramp = v,
    ),
    knob(
        5,
"CROWN SHADING",
        0.0,
        2.0,
        0.1,
        0,
        "Leaves darker near the ground and lighter toward the top of the crown, by height above the terrain. Gives a canopy depth the lighting alone misses.",
        |_, g| g.crown_shade,
        |_, g, v| g.crown_shade = v,
    ),
    // 15..20: the grade.
    knob(
        5,
"EXPOSURE",
        0.2,
        2.5,
        0.02,
        0,
        "Brightness going into the tone map. Raising it both lifts the picture and pushes more of it into the shoulder, where highlights flatten.",
        |_, g| g.exposure,
        |_, g, v| g.exposure = v,
    ),
    knob(
        3,
"TINT STRENGTH",
        0.0,
        2.0,
        0.05,
        0,
        "How strongly the TINT strip multiplies the frame. It keeps each pixel's own hue, so this stays subtle however far you push it.",
        |_, g| g.tone,
        |_, g, v| g.tone = v,
    ),
    knob(
        4,
"GRADIENT MAP",
        0.0,
        1.0,
        0.05,
        0,
        "How much the GRADIENT MAP strip replaces the frame's hue. At 1 every pixel takes the strip's colour for its brightness, and the picture goes flat.",
        |_, g| g.map_amount,
        |_, g, v| g.map_amount = v,
    ),
    knob(
        5,
"SATURATION",
        0.0,
        2.0,
        0.02,
        0,
        "Colour against grey, applied after both strips. 0 is black and white.",
        |_, g| g.saturation,
        |_, g, v| g.saturation = v,
    ),
    knob(
        5,
"CONTRAST",
        0.0,
        1.0,
        0.05,
        0,
        "An S-curve that darkens the lower half and lifts the upper. Small amounts; it bites quickly after the tone map has already rolled the ends off.",
        |_, g| g.contrast,
        |_, g, v| g.contrast = v,
    ),
    knob(
        5,
"VIGNETTE",
        0.0,
        0.8,
        0.02,
        0,
        "Darkening toward the corners, by distance from the middle of the frame.",
        |_, g| g.vignette,
        |_, g, v| g.vignette = v,
    ),
];

/// Move knob `i` one step and keep it in range.
pub fn step_knob(look: &mut Look, grade: &mut Grade, i: u8, dir: i32) {
    let Some(k) = KNOBS.get(i as usize) else {
        return;
    };
    let v = num_of(k).nudge((k.reach.get)(look, grade), dir);
    (k.reach.set)(look, grade, v);
}

pub fn page_title(page: u8) -> &'static str {
    match page {
        1 => "LIGHT",
        2 => "DISTANCE RAMP",
        3 => "TINT",
        4 => "GRADIENT MAP",
        5 => "GRADE",
        _ => "LOOK AND COLOUR",
    }
}

pub fn page_foot(page: u8) -> &'static str {
    match page {
        0 => "Enter to open, Esc to go back",
        2..=4 => "Enter on a colour to pick it, left and right to move a stop",
        _ => "Left and right, or a click, to change",
    }
}

/// What a swatch is called, for the picker's heading.
pub fn swatch_name(code: u8) -> String {
    match stop_of(code) {
        Some((ROW_MAP, i)) => format!("MAP STOP {}", i + 1),
        Some((ROW_TINT, i)) => format!("TINT STOP {}", i + 1),
        Some((_, i)) => format!("RAMP STOP {}", i + 1),
        None => match code {
            SW_ZENITH => "SKY ZENITH".into(),
            SW_HORIZON => "SKY HORIZON".into(),
            SW_GROUND => "GROUND BOUNCE".into(),
            SW_AMBIENT => "AMBIENT FILL".into(),
            _ => "SUN TINT".into(),
        },
    }
}

/// The linear colour a swatch code points at.
pub fn swatch_get(look: &Look, grade: &Grade, code: u8) -> [f32; 3] {
    if let Some((row, i)) = stop_of(code) {
        return grade.stops(row).get(i).map(|s| s.color).unwrap_or([1.0; 3]);
    }
    match code {
        SW_ZENITH => look.zenith,
        SW_HORIZON => look.horizon,
        SW_GROUND => look.ground,
        SW_AMBIENT => look.ambient,
        _ => look.sun_tint.to_array(),
    }
}

pub fn swatch_set(look: &mut Look, grade: &mut Grade, code: u8, c: [f32; 3]) {
    if let Some((row, i)) = stop_of(code) {
        if let Some(s) = grade.stops_mut(row).get_mut(i) {
            s.color = c;
        }
        return;
    }
    match code {
        SW_ZENITH => look.zenith = c,
        SW_HORIZON => look.horizon = c,
        SW_GROUND => look.ground = c,
        SW_AMBIENT => look.ambient = c,
        _ => look.sun_tint = Vec3::from(c),
    }
}

/// Which strip a swatch code's base names.
pub fn strip_of(base: u8) -> usize {
    stop_of(base).map_or(ROW_RAMP, |(row, _)| row)
}

/// The stop before the widest gap on a strip, where a new stop goes.
pub fn widest_gap(grade: &Grade, row: usize) -> usize {
    let s = grade.stops(row);
    let gap = |i: &usize| s[i + 1].at - s[*i].at;
    (0..s.len().saturating_sub(1)).max_by(|a, b| gap(a).total_cmp(&gap(b))).unwrap_or(0)
}

/// The middle stop nearest the halfway mark, which TAKE A STOP OUT removes.
/// The two ends stay, so a strip always has something to interpolate.
pub fn middle_stop(grade: &Grade, row: usize) -> usize {
    let s = grade.stops(row);
    (1..s.len().saturating_sub(1)).min_by(|a, b| (s[*a].at - 0.5).abs().total_cmp(&(s[*b].at - 0.5).abs())).unwrap_or(1)
}

/// How far one press moves a stop on the distance ramp, in metres: round
/// numbers at the scale it is already at.
fn metre_step(d: f32) -> f32 {
    match d {
        d if d < 120.0 => 5.0,
        d if d < 400.0 => 10.0,
        d if d < 1000.0 => 25.0,
        _ => 50.0,
    }
}

/// Move a stop along its strip; the stops are kept in order. On the distance
/// ramp the step is in metres, so the number on the line moves by a round
/// amount whatever shape the ramp is on.
pub fn move_stop(grade: &mut Grade, code: u8, dir: i32) -> u8 {
    let Some((row, i)) = stop_of(code) else {
        return code;
    };
    let next = if row == ROW_RAMP {
        let at = grade.stops(row)[i].at;
        // A stop at the far end has no finite distance to step from, so come
        // back in from well past the reach.
        let d = grade.ramp_distance(at).unwrap_or(grade.ramp_reach * 4.0);
        let step = metre_step(d);
        let d2 = ((d / step).round() + dir as f32) * step;
        grade.ramp_t(d2.max(0.0)).clamp(0.0, 1.0)
    } else {
        (grade.stops(row)[i].at + 0.02 * dir as f32).clamp(0.0, 1.0)
    };
    let Some(s) = grade.stops_mut(row).get_mut(i) else {
        return code;
    };
    s.at = next;
    let moved = *s;
    grade.sort();
    // The stop may have passed a neighbour: follow it to its new place.
    let at = grade.stops(row).iter().position(|s| *s == moved).unwrap_or(i) as u8;
    base_of(row) + at
}

/// The lines of one page.
pub fn items(page: u8, look: &Look, grade: &Grade) -> Vec<Item> {
    let mut out = Vec::new();
    let knobs = |out: &mut Vec<Item>, page: u8| {
        for (i, k) in KNOBS.iter().enumerate().filter(|(_, k)| k.page == page) {
            let mut it = Item::value(k.label, Choice::Knob(i as u8), num_of(k).text((k.reach.get)(look, grade)));
            it.help = k.help;
            out.push(it);
        }
    };
    let swatch = |out: &mut Vec<Item>, code: u8, label: &str, help: &'static str| {
        let c = swatch_get(look, grade, code);
        let mut it = Item::value(label, Choice::Stop(code), hex_of(c));
        it.swatch = Some(c);
        it.help = help;
        out.push(it);
    };
    let strip = |out: &mut Vec<Item>, row: usize, base: u8| {
        for (i, s) in grade.stops(row).iter().enumerate() {
            let c = s.color;
            // The distance ramp says where a stop falls in metres; the two
            // luminance strips are brightness, so they stay a fraction.
            let at = if row == ROW_RAMP {
                match grade.ramp_distance(s.at) {
                    Some(d) => format!("{d:.0} m"),
                    None => "far".to_string(),
                }
            } else {
                format!("{:.2}", s.at)
            };
            let mut it = Item::value(&format!("STOP {}", i + 1), Choice::Stop(base + i as u8), format!("{at}  {}", hex_of(c)));
            it.swatch = Some(c);
            it.help = if row == ROW_RAMP {
                "Left and right move this stop in metres; Enter picks its colour. The number is where it falls in the world, worked out from RAMP REACH and RAMP CURVE."
            } else {
                "Left and right move this stop along the brightness it answers to, 0 the darkest pixels and 1 the brightest; Enter picks its colour."
            };
            out.push(it);
        }
        let mut add = Item::new("ADD A STOP", Choice::AddStop(base));
        add.help = "Put a new stop in the widest gap, taking the colour halfway between its neighbours. Up to eight on a strip.";
        out.push(add);
        if grade.stops(row).len() > 2 {
            let mut del = Item::new("TAKE A STOP OUT", Choice::DelStop(base));
            del.help = "Take out the middle stop nearest the halfway mark. The two ends always stay.";
            out.push(del);
        }
    };
    match page {
        1 => {
            swatch(
                &mut out,
                SW_ZENITH,
                "SKY ZENITH",
                "The sky straight overhead. The clock shifts it through the day, so this is the colour it is shifted away from, not what you see now.",
            );
            swatch(
                &mut out,
                SW_HORIZON,
                "SKY HORIZON",
                "The sky at eye level. The distance ramp is held to this colour's brightness when HELD TO SKY VALUE is 1.",
            );
            swatch(
                &mut out,
                SW_GROUND,
                "GROUND BOUNCE",
                "Light coming back up off the ground into the underside of things. Keep it dark and in the soil's family.",
            );
            swatch(
                &mut out,
                SW_AMBIENT,
                "AMBIENT FILL",
                "The sky's fill on everything the sun does not reach. This is what shadows are made of, and the single biggest lever on the mood.",
            );
            swatch(
                &mut out,
                SW_SUN,
                "SUN TINT",
                "A multiplier on the key light's own colour, so the sun can be warmed without moving the sky.",
            );
            knobs(&mut out, 1);
        }
        2 => {
            strip(&mut out, ROW_RAMP, SW_RAMP);
            knobs(&mut out, 2);
        }
        3 => {
            strip(&mut out, ROW_TINT, SW_TINT);
            knobs(&mut out, 3);
        }
        4 => {
            strip(&mut out, ROW_MAP, SW_MAP);
            knobs(&mut out, 4);
        }
        5 => {
            knobs(&mut out, 5);
        }
        _ => {
            let page = |label: &str, choice: Choice, help: &'static str| {
                let mut it = Item::new(label, choice);
                it.help = help;
                it
            };
            out.push(page("LIGHT", Choice::Look(1), "Sky, ground bounce, fill and sun: the colours the world is lit by, before any grade."));
            out.push(page(
                "DISTANCE RAMP",
                Choice::Look(2),
                "A colour strip read by how far away a thing is. It colours the haze, and can colour the low sky and the shadows. This is the one that makes ridges recede through a designed sequence of hues.",
            ));
            out.push(page(
                "TINT",
                Choice::Look(3),
                "A colour strip read by brightness, multiplied into the finished frame. It tints shadows one way and highlights another and keeps every local hue.",
            ));
            out.push(page(
                "GRADIENT MAP",
                Choice::Look(4),
                "The same kind of strip, also read by brightness, but it replaces the hue instead of multiplying it. Push it and the picture flattens onto the strip's colours.",
            ));
            out.push(page("GRADE", Choice::Look(5), "Exposure, saturation, contrast and vignette: what is left after the strips."));
            out.push(page(
                "EXPORT TO THE LOOK FILE",
                Choice::Export,
                "Write everything on these pages to library/looks/<style>.json, keeping its notes: this is the style's own feel, in every world. Anything this world grades for itself stays with the world and is left as the file has it.",
            ));
            out.push(page(
                "EXPORT TO THIS WORLD",
                Choice::ExportWorld(0),
                "Write what differs from the style file to worlds/<world>/look.json, so this world keeps this colour in both styles and the styles keep their own feel underneath. Grade one world like a film: its own distance ramp, its own haze, its own gradient map.",
            ));
            out.push(page(
                "EXPORT TO THIS WORLD, THIS STYLE",
                Choice::ExportWorld(1),
                "The same, but under this style's name alone, so the world is graded one way in SATURATED and another in CLASSIC.",
            ));
            out.push(page("READ THE LOOK FILE AGAIN", Choice::ResetLook, "Throw away every change made here and go back to what is on disk."));
        }
    }
    out.push(Item::new("BACK", Choice::Back));
    out
}

/// The picker's own three lines.
pub fn pick_items(hsv: [f32; 3], dist: Option<f32>) -> Vec<Item> {
    let row = |label: &str, choice: Choice, v: String, help: &'static str| {
        let mut it = Item::value(label, choice, v);
        it.help = help;
        it
    };
    let mut out = vec![
        row(
            "HUE",
            Choice::Pick(0),
            format!("{:.0}", hsv[0]),
            "Which colour it is, round the wheel. Drag the bar under the square, or step it here.",
        ),
        row(
            "SATURATION",
            Choice::Pick(1),
            format!("{:.3}", hsv[1]),
            "How much colour against grey. Left to right across the square.",
        ),
        row(
            "VALUE",
            Choice::Pick(2),
            format!("{:.3}", hsv[2]),
            "How bright. Top to bottom of the square; it reaches past white, since tint stops sit a little above it.",
        ),
    ];
    // Only a stop on the distance ramp has a distance.
    if let Some(d) = dist {
        out.push(row(
            "DISTANCE",
            Choice::Pick(3),
            format!("{d:.0} m"),
            "How far out this stop falls. Drag the bottom bar, or step it here; the tick on the bar is whatever the cursor is pointing at.",
        ));
    }
    out.push(Item::new("DONE", Choice::PickDone));
    out
}

/// Step one of the picker's three numbers.
pub fn step_pick(hsv: &mut [f32; 3], which: u8, dir: i32) {
    match which {
        0 => hsv[0] = (hsv[0] + 4.0 * dir as f32).rem_euclid(360.0),
        1 => hsv[1] = (hsv[1] + 0.02 * dir as f32).clamp(0.0, 1.0),
        _ => hsv[2] = (hsv[2] + 0.02 * dir as f32).clamp(0.0, V_MAX),
    }
}

// ---- colour ----------------------------------------------------------------

/// Highest value the picker reaches. Tint stops sit a little above white, so
/// the square has to go past 1.
pub const V_MAX: f32 = 1.3;

fn to_srgb(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_linear(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// A linear colour as hue (degrees), saturation and value, worked out on the
/// sRGB-encoded numbers so the picker moves the way the eye expects.
pub fn to_hsv(c: [f32; 3]) -> [f32; 3] {
    let s: [f32; 3] = std::array::from_fn(|i| to_srgb(c[i]));
    let max = s[0].max(s[1]).max(s[2]);
    let min = s[0].min(s[1]).min(s[2]);
    let d = max - min;
    let h = if d < 1e-6 {
        0.0
    } else if max == s[0] {
        60.0 * (((s[1] - s[2]) / d) % 6.0)
    } else if max == s[1] {
        60.0 * ((s[2] - s[0]) / d + 2.0)
    } else {
        60.0 * ((s[0] - s[1]) / d + 4.0)
    };
    [h.rem_euclid(360.0), if max < 1e-6 { 0.0 } else { d / max }, max.min(V_MAX)]
}

/// The linear colour for a hue, saturation and value.
pub fn from_hsv(hsv: [f32; 3]) -> [f32; 3] {
    let (h, s, v) = (hsv[0].rem_euclid(360.0) / 60.0, hsv[1].clamp(0.0, 1.0), hsv[2].max(0.0));
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    let srgb = match i as i32 % 6 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    };
    std::array::from_fn(|k| to_linear(srgb[k]))
}

/// A linear colour written the way it would be typed: #rrggbb, with a "+"
/// after it when a channel is brighter than white.
pub fn hex_of(c: [f32; 3]) -> String {
    let s: [f32; 3] = std::array::from_fn(|i| to_srgb(c[i]));
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    let over = if s.iter().any(|v| *v > 1.001) { "+" } else { "" };
    format!("#{:02X}{:02X}{:02X}{over}", b(s[0]), b(s[1]), b(s[2]))
}

// ---- drawing ---------------------------------------------------------------

/// A colour the grade already holds in linear, made opaque for the HUD.
fn opaque(c: [f32; 3]) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

/// Where the picker's parts sit: the saturation-value square, the hue bar
/// and the distance bar, in the bottom right corner, small enough to leave
/// the frame to look at. The distance bar is only live on a ramp stop, but
/// the room for it is always kept so the panel does not jump.
pub fn picker_rects(w: f32, h: f32) -> ([f32; 4], [f32; 4], [f32; 4]) {
    let u = menu::unit(w, h);
    let side = 14.0 * u;
    let bar = 1.8 * u;
    let x = w - side - 4.0 * u;
    // Laid out from the bottom up: distance bar, swatch row, hue bar, square.
    let slider_y = h - 3.6 * u - bar;
    let hue_y = slider_y - 1.8 * u - 3.0 * u - 1.4 * u - bar;
    let sq_y = hue_y - 1.2 * u - side;
    ([x, sq_y, side, side], [x, hue_y, side, bar], [x, slider_y, side, bar])
}

/// The page: a form down the left (src/form.rs), the same panel the EDIT
/// page has on the right, with the world showing beside it.
pub fn draw(hud: &mut Hud, m: &mut menu::Menu, heading: &str, items: &[Item], foot: &str, w: f32, h: f32) {
    let rows = rows(items);
    let buttons = rows.iter().rev().take_while(|r| r.field == Field::Button).count();
    let f = Form {
        title: heading,
        foot,
        head: 0,
        buttons,
        side: Side::Left,
        // The LOOK page's help says what a knob does to the picture, which
        // takes more than the EDIT page's two lines.
        help_lines: 4,
        squeeze: true,
    };
    let (mut top, mut shown) = (m.top, m.shown);
    form::draw(hud, m, &rows, &f, &mut top, &mut shown, w, h);
    (m.top, m.shown) = (top, shown);
}

/// Each line as a form row. How a line is worked decides what stands on its
/// end: arrows round a number, a chip and a hex for a colour, a word for a
/// line that does something.
fn rows(items: &[Item]) -> Vec<Row> {
    items
        .iter()
        .map(|it| {
            let chip = opaque(it.swatch.unwrap_or_default());
            let field = match it.choice {
                Choice::Back | Choice::PickDone => Field::Button,
                Choice::Stop(code) if code >= SW_RAMP => Field::Stop(chip),
                Choice::Stop(_) => Field::Colour(chip),
                _ if it.drop => Field::List,
                _ if it.value.is_some() => Field::Step,
                Choice::Look(_) => Field::Go("OPEN"),
                Choice::Export | Choice::ExportWorld(_) => Field::Go("WRITE"),
                Choice::ResetLook => Field::Go("READ"),
                Choice::AddStop(_) => Field::Go("ADD"),
                Choice::DelStop(_) => Field::Go("TAKE"),
                _ => Field::Plain,
            };
            Row::new(it.clone(), field)
        })
        .collect()
}

/// How far away the first solid voxel under `cursor` is, in metres. The LOOK
/// page shows it so a ramp stop can be put where a ridge actually is.
pub fn probe(game: &crate::game::Game, cursor: (f32, f32), w: f32, h: f32) -> Option<f32> {
    let p = game.frame_params(w / h.max(1.0));
    // The cursor as a ray: unproject a point on the near plane (z = 1 in this
    // reversed infinite projection) and look along it.
    let ndc = Vec3::new(cursor.0 / w * 2.0 - 1.0, 1.0 - cursor.1 / h.max(1.0) * 2.0, 1.0);
    let on_near = (p.proj * p.view).inverse().project_point3(ndc);
    let dir = (on_near - p.cam_pos).normalize_or_zero();
    if dir == Vec3::ZERO {
        return None;
    }
    // Two passes, nearest wins. The voxel store has everything that is drawn
    // (trunks, rock, walls) but only out to the streamed radius; the generated
    // terrain height has the ground at any range but nothing standing on it.
    // The ramp itself colours whatever surface is drawn, so the near pass is
    // the one that agrees with it, and the far pass only answers where the
    // store has nothing to say.
    let near = game.store.raycast(p.cam_pos, dir, 2500.0).map(|(hit, _)| {
        let centre = Vec3::new(hit[0] as f32 + 0.5, hit[1] as f32 + 0.5, hit[2] as f32 + 0.5) * crate::world::gen::VOXEL;
        (centre - p.cam_pos).length()
    });
    let gen = game.generator();
    let above = |t: f32| {
        let q = p.cam_pos + dir * t;
        q.y - gen.height(q.x, q.z)
    };
    const FAR: f32 = 3000.0;
    const STEP: f32 = 4.0;
    let ground = if above(0.0) < 0.0 {
        Some(0.0)
    } else {
        let mut found = None;
        let mut t = STEP;
        while t < FAR {
            if above(t) < 0.0 {
                // Between the last step and this one; close in on the crossing.
                let (mut lo, mut hi) = (t - STEP, t);
                for _ in 0..12 {
                    let mid = (lo + hi) * 0.5;
                    if above(mid) < 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                found = Some((lo + hi) * 0.5);
                break;
            }
            t += STEP;
        }
        found
    };
    match (near, ground) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// How far out a gradient stop falls, for the stops on the distance ramp.
pub fn stop_distance(grade: &Grade, code: u8) -> Option<f32> {
    let (row, i) = stop_of(code)?;
    if row != ROW_RAMP {
        return None;
    }
    grade.ramp_distance(grade.stops(row).get(i)?.at)
}

/// Put a ramp stop at `metres` and return where it ended up, since it may
/// have passed a neighbour.
pub fn set_stop_distance(grade: &mut Grade, code: u8, metres: f32) -> u8 {
    let Some((row, i)) = stop_of(code) else {
        return code;
    };
    if row != ROW_RAMP {
        return code;
    }
    let at = grade.ramp_t(metres.max(0.0)).clamp(0.0, 1.0);
    let Some(s) = grade.stops_mut(row).get_mut(i) else {
        return code;
    };
    s.at = at;
    let moved = *s;
    grade.sort();
    base_of(row) + grade.stops(row).iter().position(|s| *s == moved).unwrap_or(i) as u8
}

/// The far end of the picker's distance slider: twice the reach, so the whole
/// useful range of the ramp is on the bar.
pub fn slider_far(grade: &Grade) -> f32 {
    grade.ramp_reach * 2.0
}

/// How far away whatever is under the cursor is, and the colour the distance
/// ramp gives at that range: the quickest way to put a stop where a ridge is.
pub fn draw_probe(hud: &mut Hud, grade: &Grade, dist: f32, cursor: (f32, f32), w: f32, h: f32) {
    let u = menu::unit(w, h);
    let p = menu::pal();
    let t = Type::new(Face::Caps, 1.16 * u, 0.1);
    let text = format!("{dist:.0} M");
    let tw = Hud::measure(t, &text);
    let chip = 1.6 * u;
    let bw = tw + chip + 2.4 * u;
    let bh = 2.4 * u;
    // Beside the cursor, flipped away from the edges of the frame.
    let x = (cursor.0 + 1.6 * u).min(w - bw - 1.0 * u).max(0.0);
    let y = (cursor.1 - bh - 1.2 * u).max(1.0 * u);
    hud.rect(x, y, bw, bh, [0.02, 0.02, 0.03, 0.55]);
    let c = crate::render::grade::sample(grade.stops(ROW_RAMP), grade.ramp_t(dist));
    hud.rect(x + 0.7 * u, y + (bh - chip) * 0.5, chip, chip, opaque(c));
    hud.set_lit(t, x + 0.7 * u + chip + 0.7 * u, y + bh * 0.5 + t.cap() * 0.5, Ink::Flat(p.ink_value), 0.7, &text);
    // A small cross on the point itself.
    let k = (0.14 * u).max(1.0);
    let r = 0.9 * u;
    hud.rect(cursor.0 - r, cursor.1 - k * 0.5, r * 2.0, k, [1.0, 1.0, 1.0, 0.8]);
    hud.rect(cursor.0 - k * 0.5, cursor.1 - r, k, r * 2.0, [1.0, 1.0, 1.0, 0.8]);
}

/// The picker: a saturation-value square under the chosen hue, a hue strip
/// below it, and the colour itself beside them.
pub fn draw_picker(hud: &mut Hud, hsv: [f32; 3], name: &str, bar: Option<DistBar>, w: f32, h: f32) {
    let u = menu::unit(w, h);
    let p = menu::pal();
    let (sq, hb, sl) = picker_rects(w, h);
    let ht = Type::new(Face::CapsBold, 1.3 * u, 0.12);
    hud.set_lit(ht, sq[0], sq[1] - 1.2 * u, Ink::Flat(p.note), 0.8, name);
    // The square: strips of rising saturation, each shading from full value
    // at the top down to black.
    let n = 40;
    let sw = sq[2] / n as f32;
    for i in 0..n {
        let s = (i as f32 + 0.5) / n as f32;
        let topc = opaque(from_hsv([hsv[0], s, V_MAX]));
        hud.grad_v((sq[0] + i as f32 * sw).floor(), sq[1], sw.ceil(), sq[3], topc, [0.0, 0.0, 0.0, 1.0]);
    }
    // The hue strip, in twelve shaded steps.
    for i in 0..12 {
        let a = opaque(from_hsv([i as f32 * 30.0, 1.0, 1.0]));
        let b = opaque(from_hsv([(i + 1) as f32 * 30.0, 1.0, 1.0]));
        hud.grad_h((hb[0] + hb[2] * i as f32 / 12.0).floor(), hb[1], (hb[2] / 12.0).ceil(), hb[3], a, b);
    }
    // Where we are: a ring in the square, a bar on the strip.
    let mx = sq[0] + hsv[1].clamp(0.0, 1.0) * sq[2];
    let my = sq[1] + (1.0 - (hsv[2] / V_MAX).clamp(0.0, 1.0)) * sq[3];
    let t = (0.16 * u).max(1.0);
    let r = 0.7 * u;
    for (c, k) in [([0.0, 0.0, 0.0, 0.8], 1.0), ([1.0, 1.0, 1.0, 0.95], 0.0)] {
        let rr = r + t * k;
        hud.rect(mx - rr, my - rr, rr * 2.0, t, c);
        hud.rect(mx - rr, my + rr - t, rr * 2.0, t, c);
        hud.rect(mx - rr, my - rr, t, rr * 2.0, c);
        hud.rect(mx + rr - t, my - rr, t, rr * 2.0, c);
    }
    let hx = hb[0] + hsv[0] / 360.0 * hb[2];
    hud.rect(hx - t, hb[1] - t, t * 3.0, hb[3] + t * 2.0, [1.0, 1.0, 1.0, 0.95]);
    // The colour itself, with what it would be typed as, under the strip.
    let c = from_hsv(hsv);
    let bs = 3.0 * u;
    let by = hb[1] + hb[3] + 1.4 * u;
    hud.rect(sq[0] - 1.0, by - 1.0, bs + 2.0, bs + 2.0, [0.02, 0.02, 0.03, 0.9]);
    hud.rect(sq[0], by, bs, bs, opaque(c));
    let vt = Type::new(Face::Italic, 1.4 * u, 0.01);
    hud.set_lit(vt, sq[0] + bs + 1.2 * u, by + bs * 0.5 + vt.size * 0.3, Ink::Flat(p.ink_value), 0.7, &hex_of(c));
    // How far out the stop sits, on a bar of its own.
    let Some(b) = bar else { return };
    let lt = Type::new(Face::Caps, 1.1 * u, 0.1);
    let far = b.far();
    let text = format!("{:.0} M", b.at);
    hud.set_lit(lt, sl[0], sl[1] - 0.7 * u, Ink::Flat(p.note), 0.75, &text);
    // The bar is the ramp itself, so the stop is dragged over the colours it
    // is making.
    let n = 32;
    let sw = sl[2] / n as f32;
    for i in 0..n {
        let d = (i as f32 + 0.5) / n as f32 * far;
        let col = crate::render::grade::sample(&b.grade.ramp, b.grade.ramp_t(d));
        hud.rect((sl[0] + i as f32 * sw).floor(), sl[1], sw.ceil(), sl[3], opaque(col));
    }
    let t1 = (0.16 * u).max(1.0);
    // A tick for whatever the cursor is pointing at, to drag the stop onto.
    if let Some(probe) = b.probe {
        let px = sl[0] + (probe / far).clamp(0.0, 1.0) * sl[2];
        hud.rect(px - t1 * 0.5, sl[1] - 0.6 * u, t1, sl[3] + 1.2 * u, with_a(p.apricot, 0.95));
    }
    let hx = sl[0] + (b.at / far).clamp(0.0, 1.0) * sl[2];
    hud.rect(hx - t1 * 1.5, sl[1] - t1, t1 * 3.0, sl[3] + t1 * 2.0, [0.02, 0.02, 0.03, 0.9]);
    hud.rect(hx - t1 * 0.5, sl[1] - t1 * 0.5, t1, sl[3] + t1, [1.0, 1.0, 1.0, 0.95]);
}

/// What the picker needs to draw the distance bar for a ramp stop.
pub struct DistBar {
    /// Where the stop is now, in metres.
    pub at: f32,
    /// The grade it belongs to, for the bar's own colours and its mapping
    /// from distance to a place on the strip.
    pub grade: Grade,
    /// Range to the thing under the cursor, marked on the bar.
    pub probe: Option<f32>,
}

impl DistBar {
    pub fn new(grade: &Grade, at: f32, probe: Option<f32>) -> DistBar {
        DistBar { at, grade: grade.clone(), probe }
    }

    /// The far end of the bar, in metres.
    fn far(&self) -> f32 {
        slider_far(&self.grade)
    }
}

// ---- export ----------------------------------------------------------------

/// A look file, in the order it is written.
#[derive(Serialize)]
struct LookFile<'a> {
    name: &'a str,
    notes: &'a str,
    zenith: [f32; 3],
    horizon: [f32; 3],
    ground: [f32; 3],
    ambient: [f32; 3],
    sun_tint: [f32; 3],
    sun_gain: f32,
    /// Fog density is 1 / this, in metres.
    fog_distance: f32,
    grade: &'a Grade,
}

pub fn look_path(style: Style) -> std::path::PathBuf {
    crate::library::data_root().join("library").join("looks").join(format!("{}.json", style.name()))
}

/// Write the live look and grade back to the style's look file, keeping its
/// notes. Whatever the world is grading for itself is left as the file has
/// it, since on screen that value belongs to the world, not the style: the
/// count of those is returned with the path.
pub fn export(style: Style, look: &Look, grade: &Grade, world: Option<&crate::library::WorldLook>) -> Result<(std::path::PathBuf, usize), String> {
    let (mut l, mut g) = (*look, grade.clone());
    let mut kept = 0;
    if let Some(w) = world {
        let (base_look, base_grade) = style_from_file(style).ok_or_else(|| format!("no look file for {}", style.name()))?;
        // Everything the world's own file sets, in either patch; what it
        // says nothing about is the style's and goes through.
        let mut patch = crate::library::WorldLook::default();
        if let Some(p) = w.styles.get(style.name()) {
            merge_names(&mut patch, p);
        }
        merge_names(&mut patch, w);
        kept = restore(&patch, &base_look, &base_grade, &mut l, &mut g);
    }
    let path = export_to(&look_path(style), style, &l, &g)?;
    Ok((path, kept))
}

/// Mark in `into` every field `from` sets, with `from`'s values: used to
/// collect what a world owns before the style is written.
fn merge_names(into: &mut crate::library::WorldLook, from: &crate::library::WorldLook) {
    let f = from.clone();
    into.zenith = f.zenith.or(into.zenith);
    into.horizon = f.horizon.or(into.horizon);
    into.ground = f.ground.or(into.ground);
    into.ambient = f.ambient.or(into.ambient);
    into.sun_tint = f.sun_tint.or(into.sun_tint);
    into.sun_gain = f.sun_gain.or(into.sun_gain);
    into.fog_distance = f.fog_distance.or(into.fog_distance);
    into.ramp = f.ramp.or_else(|| into.ramp.clone());
    into.tint = f.tint.or_else(|| into.tint.clone());
    into.map = f.map.or_else(|| into.map.clone());
    for (k, v) in f.knobs {
        into.knobs.insert(k, v);
    }
}

/// Put back the style file's own value for every field `patch` names, and
/// say how many it put back.
fn restore(patch: &crate::library::WorldLook, base_look: &Look, base_grade: &Grade, look: &mut Look, grade: &mut Grade) -> usize {
    let mut n = 0;
    let mut put = |yes: bool| {
        n += yes as usize;
        yes
    };
    if put(patch.zenith.is_some()) {
        look.zenith = base_look.zenith;
    }
    if put(patch.horizon.is_some()) {
        look.horizon = base_look.horizon;
    }
    if put(patch.ground.is_some()) {
        look.ground = base_look.ground;
    }
    if put(patch.ambient.is_some()) {
        look.ambient = base_look.ambient;
    }
    if put(patch.sun_tint.is_some()) {
        look.sun_tint = base_look.sun_tint;
    }
    if put(patch.sun_gain.is_some()) {
        look.sun_gain = base_look.sun_gain;
    }
    if put(patch.fog_distance.is_some()) {
        look.fog_density = base_look.fog_density;
    }
    if put(patch.ramp.is_some()) {
        grade.ramp = base_grade.ramp.clone();
    }
    if put(patch.tint.is_some()) {
        grade.tint = base_grade.tint.clone();
    }
    if put(patch.map.is_some()) {
        grade.map = base_grade.map.clone();
    }
    for name in patch.knobs.keys() {
        if let (Some(b), Some(slot)) = (base_grade.field(name), grade.field_mut(name)) {
            *slot = b;
            n += 1;
        }
    }
    n
}

fn export_to(path: &std::path::Path, style: Style, look: &Look, grade: &Grade) -> Result<std::path::PathBuf, String> {
    // Keep whatever the file says about itself; everything else is replaced.
    let notes = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v.get("notes").and_then(|n| n.as_str()).map(str::to_string))
        .unwrap_or_default();
    let out = LookFile {
        name: style.name(),
        notes: &notes,
        zenith: look.zenith,
        horizon: look.horizon,
        ground: look.ground,
        ambient: look.ambient,
        sun_tint: look.sun_tint.to_array(),
        sun_gain: look.sun_gain,
        fog_distance: 1.0 / look.fog_density.max(1e-6),
        grade,
    };
    let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    std::fs::write(path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path.to_path_buf())
}

/// The light and grade a style is drawn in: its look file, then the world's
/// own grading over it (`library::WorldLook`), then the VOXEL_GRADE study
/// switches, which stay on top so they still override everything.
pub fn of_style(d: &crate::library::LookDef, world: Option<&crate::library::WorldLook>, style: Style) -> (Look, Grade) {
    let mut look = Look {
        zenith: d.zenith,
        horizon: d.horizon,
        ground: d.ground,
        ambient: d.ambient,
        sun_tint: Vec3::from(d.sun_tint),
        sun_gain: d.sun_gain,
        fog_density: 1.0 / d.fog_distance,
    };
    let mut grade = d.grade.clone();
    if let Some(w) = world {
        w.apply(style.name(), &mut look, &mut grade);
    }
    grade.apply_env(&std::env::var("VOXEL_GRADE").unwrap_or_default());
    (look, grade)
}

/// Where a world keeps its own grading.
pub fn world_look_path(dir: &std::path::Path) -> std::path::PathBuf {
    dir.join("look.json")
}

/// Write what the page has made of this world into `worlds/<name>/look.json`,
/// as the difference from the style's own look file: what matches the style
/// is left out, so the file says only what is this world's own, and the
/// styles keep their feel under it. `shared` says whether this is the
/// world's grading in every style or only in this one.
pub fn export_world(dir: &std::path::Path, style: Style, look: &Look, grade: &Grade, shared: bool) -> Result<std::path::PathBuf, String> {
    let (base_look, base_grade) = style_from_file(style).ok_or_else(|| format!("no look file for {}", style.name()))?;
    let patch = diff(&base_look, &base_grade, look, grade);
    let path = world_look_path(dir);
    // Keep the rest of the file: the notes, and whatever belongs to the
    // other style or to every style.
    let (old, _) = crate::library::load_world_look(dir);
    let mut out = old.unwrap_or_default();
    let notes = std::mem::take(&mut out.notes);
    if shared {
        let styles = std::mem::take(&mut out.styles);
        out = patch;
        out.styles = styles;
    } else {
        let mut styles = std::mem::take(&mut out.styles);
        styles.insert(style.name().to_string(), patch);
        out.styles = styles;
    }
    out.notes = notes;
    if out.is_empty() {
        // Nothing of its own left: the world goes back to the plain styles.
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
        return Ok(path);
    }
    let text = serde_json::to_string_pretty(&out).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// What `look`/`grade` have that the style's own look file does not.
fn diff(base_look: &Look, base_grade: &Grade, look: &Look, grade: &Grade) -> crate::library::WorldLook {
    let mut out = crate::library::WorldLook::default();
    let col = |a: [f32; 3], b: [f32; 3]| (0..3).any(|i| (a[i] - b[i]).abs() > 1e-6);
    if col(look.zenith, base_look.zenith) {
        out.zenith = Some(look.zenith);
    }
    if col(look.horizon, base_look.horizon) {
        out.horizon = Some(look.horizon);
    }
    if col(look.ground, base_look.ground) {
        out.ground = Some(look.ground);
    }
    if col(look.ambient, base_look.ambient) {
        out.ambient = Some(look.ambient);
    }
    if col(look.sun_tint.to_array(), base_look.sun_tint.to_array()) {
        out.sun_tint = Some(look.sun_tint.to_array());
    }
    if (look.sun_gain - base_look.sun_gain).abs() > 1e-6 {
        out.sun_gain = Some(look.sun_gain);
    }
    if (look.fog_density - base_look.fog_density).abs() > 1e-9 {
        out.fog_distance = Some(1.0 / look.fog_density.max(1e-6));
    }
    if grade.ramp != base_grade.ramp {
        out.ramp = Some(grade.ramp.clone());
    }
    if grade.tint != base_grade.tint {
        out.tint = Some(grade.tint.clone());
    }
    if grade.map != base_grade.map {
        out.map = Some(grade.map.clone());
    }
    for name in crate::render::grade::FIELD_NAMES {
        let (a, b) = (grade.field(name), base_grade.field(name));
        if let (Some(a), Some(b)) = (a, b) {
            if (a - b).abs() > 1e-6 {
                out.knobs.insert((*name).to_string(), a);
            }
        }
    }
    out
}

/// The look and grade as the files on disk have them, for READ AGAIN: the
/// style's, with this world's grading over it.
pub fn from_file(style: Style, world: Option<&crate::library::WorldLook>) -> Option<(Look, Grade)> {
    let (looks, _) = crate::library::load_looks(&crate::library::data_root());
    let d = looks.into_iter().find(|d| Style::parse(&d.name) == Some(style))?;
    Some(of_style(&d, world, style))
}

/// The style's look file alone, without any world over it.
fn style_from_file(style: Style) -> Option<(Look, Grade)> {
    let (looks, _) = crate::library::load_looks(&crate::library::data_root());
    let d = looks.into_iter().find(|d| Style::parse(&d.name) == Some(style))?;
    let mut grade = d.grade.clone();
    grade.apply_env(&std::env::var("VOXEL_GRADE").unwrap_or_default());
    Some((
        Look {
            zenith: d.zenith,
            horizon: d.horizon,
            ground: d.ground,
            ambient: d.ambient,
            sun_tint: Vec3::from(d.sun_tint),
            sun_gain: d.sun_gain,
            fog_density: 1.0 / d.fog_distance,
        },
        grade,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An exported file reads back as the same look and grade: this is the
    /// whole point of EXPORT, so it is checked end to end on a real file.
    #[test]
    fn export_reads_back_as_what_was_written() {
        let mut look = crate::render::builtin_look(Style::Saturated);
        look.sun_gain = 1.23;
        look.zenith = [0.11, 0.37, 0.81];
        let mut grade = Grade::default();
        grade.map_amount = 0.65;
        grade.ramp_reach = 480.0;
        grade.ramp[1].color = [0.4, 0.2, 0.7];
        grade.add_stop(ROW_MAP, 0);
        let dir = std::env::temp_dir().join(format!("lurelia-look-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("saturated.json");
        std::fs::write(&path, "{ \"notes\": \"kept\" }").unwrap();
        export_to(&path, Style::Saturated, &look, &grade).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let back: crate::library::LookDef = serde_json::from_str(&text).unwrap();
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(back.notes, "kept", "the file keeps what it says about itself");
        assert_eq!(back.name, "saturated");
        assert_eq!(back.zenith, look.zenith);
        assert!((back.sun_gain - 1.23).abs() < 1e-6);
        assert!((1.0 / back.fog_distance - look.fog_density).abs() < 1e-9);
        assert_eq!(back.grade, grade);
    }

    /// A world's own grading is the difference from the style: what the page
    /// changed comes back in the patch, what it left alone does not, and
    /// applying the patch to the style's look reproduces what was on screen.
    #[test]
    fn a_world_patch_is_the_difference_and_puts_it_back() {
        let base_look = crate::render::builtin_look(Style::Saturated);
        let base_grade = Grade::default();
        let mut look = base_look;
        let mut grade = base_grade.clone();
        look.sun_gain = 1.4;
        grade.ramp_reach = 1200.0;
        grade.saturation = 0.8;
        grade.ramp = vec![crate::render::grade::Stop::new(0.0, [0.9, 0.4, 0.2]), crate::render::grade::Stop::new(1.0, [0.2, 0.3, 0.5])];

        let patch = diff(&base_look, &base_grade, &look, &grade);
        assert_eq!(patch.sun_gain, Some(1.4));
        assert_eq!(patch.knobs.get("ramp_reach"), Some(&1200.0));
        assert_eq!(patch.knobs.get("saturation"), Some(&0.8));
        assert!(!patch.knobs.contains_key("haze_height"), "an untouched knob is the style's: {:?}", patch.knobs);
        assert!(patch.zenith.is_none(), "an untouched colour is the style's");
        assert_eq!(patch.ramp.as_ref(), Some(&grade.ramp));
        assert!(patch.map.is_none());
        assert!(patch.problems().is_empty(), "{:?}", patch.problems());

        let (mut back_look, mut back_grade) = (base_look, base_grade.clone());
        patch.apply("saturated", &mut back_look, &mut back_grade);
        assert_eq!(back_look.sun_gain, look.sun_gain);
        assert_eq!(back_grade, grade, "the patch over the style is what was on screen");

        // A patch under one style's name reaches that style only.
        let mut only = crate::library::WorldLook::default();
        only.styles.insert("classic".to_string(), patch.clone());
        let (mut l, mut g) = (base_look, base_grade.clone());
        only.apply("saturated", &mut l, &mut g);
        assert_eq!(g, base_grade, "saturated is untouched by a classic-only patch");
        only.apply("classic", &mut l, &mut g);
        assert_eq!(g.ramp_reach, 1200.0);
    }

    /// Writing the style file while a world is grading itself leaves the
    /// world's own values alone: on screen they belong to the world, so the
    /// style would otherwise swallow them.
    #[test]
    fn the_style_export_leaves_the_worlds_own_values_to_the_world() {
        let base_look = crate::render::builtin_look(Style::Saturated);
        let base_grade = Grade::default();
        let mut world = crate::library::WorldLook::default();
        world.knobs.insert("ramp_reach".to_string(), 1500.0);
        world.sun_gain = Some(1.4);

        // On screen: the world's two values, plus one the player moved that
        // the world says nothing about.
        let (mut look, mut grade) = (base_look, base_grade.clone());
        world.apply("saturated", &mut look, &mut grade);
        grade.contrast = 0.4;

        let mut patch = crate::library::WorldLook::default();
        merge_names(&mut patch, &world);
        let (mut out_look, mut out_grade) = (look, grade.clone());
        let kept = restore(&patch, &base_look, &base_grade, &mut out_look, &mut out_grade);
        assert_eq!(kept, 2, "the two the world owns");
        assert_eq!(out_grade.ramp_reach, base_grade.ramp_reach, "the world keeps its reach");
        assert_eq!(out_look.sun_gain, base_look.sun_gain, "the world keeps its sun");
        assert_eq!(out_grade.contrast, 0.4, "what the world says nothing about goes to the style");
    }

    /// The whole path on a real folder: export to a world, read it back,
    /// and an export with nothing of its own takes the file away again.
    #[test]
    fn a_world_look_file_round_trips_and_clears() {
        let base = style_from_file(Style::Saturated).expect("the saturated look file");
        let dir = std::env::temp_dir().join(format!("lurelia-world-look-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut grade = base.1.clone();
        grade.haze_height = 42.0;
        export_world(&dir, Style::Saturated, &base.0, &grade, true).unwrap();
        let (read, problems) = crate::library::load_world_look(&dir);
        assert!(problems.is_empty(), "{problems:?}");
        let read = read.expect("a world look was written");
        assert_eq!(read.knobs.get("haze_height"), Some(&42.0));
        let (mut l, mut g) = (base.0, base.1.clone());
        read.apply("saturated", &mut l, &mut g);
        assert_eq!(g.haze_height, 42.0);

        export_world(&dir, Style::Saturated, &base.0, &base.1, true).unwrap();
        assert!(!world_look_path(&dir).exists(), "a world grading nothing keeps no file");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A knob nothing is called, and a style that is not a style, are
    /// reported rather than ignored.
    #[test]
    fn a_misspelt_knob_is_reported() {
        let mut w = crate::library::WorldLook::default();
        w.knobs.insert("haze_hieght".to_string(), 10.0);
        w.styles.insert("poster".to_string(), crate::library::WorldLook::default());
        let p = w.problems();
        assert!(p.iter().any(|x| x.contains("haze_hieght")), "{p:?}");
        assert!(p.iter().any(|x| x.contains("poster")), "{p:?}");
    }

    #[test]
    fn hsv_round_trips() {
        for c in [[0.13f32, 0.38, 0.78], [0.98, 0.82, 0.56], [1.07, 1.0, 0.86], [0.5, 0.5, 0.5], [0.0, 0.0, 0.0]] {
            let back = from_hsv(to_hsv(c));
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 0.002, "{c:?} -> {:?} -> {back:?}", to_hsv(c));
            }
        }
    }

    #[test]
    fn every_knob_reads_back_what_it_wrote() {
        let mut look = crate::render::builtin_look(Style::Saturated);
        let mut grade = Grade::default();
        for (i, k) in KNOBS.iter().enumerate() {
            let (get, set, n) = (k.reach.get, k.reach.set, num_of(k));
            set(&mut look, &mut grade, n.hi);
            let got = get(&look, &grade);
            assert!((got - n.hi).abs() < n.hi.abs() * 1e-3 + 1e-4, "{}: set {} read {got}", k.label, n.hi);
            step_knob(&mut look, &mut grade, i as u8, 1);
            assert!(get(&look, &grade) <= n.hi + 1e-4, "{} went past its top", k.label);
            set(&mut look, &mut grade, n.lo);
            step_knob(&mut look, &mut grade, i as u8, -1);
            assert!(get(&look, &grade) >= n.lo - 1e-4, "{} went below its bottom", k.label);
        }
    }

    /// Every knob is on one of the five group pages, so none is out of reach.
    /// The pages used to be lists of indices, and a knob put in the middle
    /// of the table moved three knobs onto the wrong page and left VIGNETTE
    /// on none.
    #[test]
    fn every_knob_is_on_a_page() {
        let look = crate::render::builtin_look(Style::Saturated);
        let grade = Grade::default();
        let shown: Vec<u8> = (1..=5u8)
            .flat_map(|p| items(p, &look, &grade))
            .filter_map(|it| match it.choice {
                Choice::Knob(i) => Some(i),
                _ => None,
            })
            .collect();
        for (i, k) in KNOBS.iter().enumerate() {
            assert!(shown.contains(&(i as u8)), "{} is on no page", k.label);
        }
        assert!(KNOBS.iter().all(|k| (1..=5).contains(&k.page)));
    }

    /// Every page's lines point at something that exists.
    #[test]
    fn pages_are_well_formed() {
        let look = crate::render::builtin_look(Style::Saturated);
        let grade = Grade::default();
        for page in 0..=5u8 {
            let items = items(page, &look, &grade);
            assert!(!items.is_empty());
            assert_eq!(items.last().map(|i| i.choice), Some(Choice::Back), "page {page} ends in BACK");
            for it in &items {
                match it.choice {
                    Choice::Knob(i) => assert!((i as usize) < KNOBS.len()),
                    Choice::Stop(code) => {
                        swatch_get(&look, &grade, code);
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn a_stop_keeps_its_colour_when_it_passes_a_neighbour() {
        let mut g = Grade::default();
        let want = g.ramp[0].color;
        let mut code = SW_RAMP;
        for _ in 0..40 {
            code = move_stop(&mut g, code, 1);
        }
        assert!(g.ramp.windows(2).all(|w| w[0].at <= w[1].at), "stops stay in order: {:?}", g.ramp);
        let i = (code - SW_RAMP) as usize;
        assert_eq!(g.ramp[i].color, want, "the stop we are moving is still the one we picked");
    }
}
