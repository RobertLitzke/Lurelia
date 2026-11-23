//! The race's own launch screens: a card
//! for each course, a card for each kart, then the race's own options, in
//! steps that Enter moves through and Esc backs out of. main.rs owns the
//! steps (`Screen::Race`) and what each choice does; this reads what the
//! cards show and draws them.

use crate::launch::{Launch, Opt};
use crate::menu::{border, button, button_height, button_type, heading_type, italic, line_type, pal, soften, unit, Choice, Item, Menu};
use crate::race::clock;
use crate::race::records::Records;
use crate::render::font::Face;
use crate::render::hud::{with_a, Hud, Ink, Type};
use glam::Vec2;

/// The game every card here is for (`library/games/race.json`).
pub const GAME: &str = "race";

/// The steps, as the line across the top names them.
const STEPS: [&str; 3] = ["COURSE", "KART", "RACE"];

/// What a course card shows, read from the world's files without loading it.
pub struct Course {
    pub world: String,
    pub title: String,
    pub blurb: String,
    /// The circuit's centreline, a couple of metres a point, in (x, z).
    pub line: Vec<Vec2>,
    pub length: f32,
    pub laps: u32,
    pub rivals: u32,
    /// Places the circuit passes over or under itself.
    pub crossings: usize,
}

/// Every world with a race, as cards. Read once: the files don't change
/// under a running game.
pub fn courses() -> &'static [Course] {
    static C: std::sync::OnceLock<Vec<Course>> = std::sync::OnceLock::new();
    C.get_or_init(|| crate::launch::worlds_for(GAME).iter().filter_map(|w| course(w)).collect())
}

/// Put the launch on a course: its world, raced.
pub fn set_course(l: &mut Launch, world: &str) {
    l.set_world(world);
    l.set_game(GAME);
}

fn course(world: &str) -> Option<Course> {
    let dir = crate::worldload::world_dir(world);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    let wj = read("world.json")?;
    let race = wj.get("race")?;
    let name = race.get("circuit").and_then(|c| c.as_str()).unwrap_or("");
    // The laps and rivals it is written for, as the launch screen read them.
    let info = crate::launch::info(world);
    let xz = |p: &serde_json::Value| {
        let a = p.as_array()?;
        Some(Vec2::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
    };
    let line = match race.get("track").and_then(|t| t.as_str()) {
        // A track file's line is a point a metre round: every other one.
        Some(file) => {
            let t = read(file)?;
            let pts: Vec<Vec2> = t.get("line")?.as_array()?.iter().step_by(2).filter_map(xz).collect();
            if pts.len() < 3 {
                return None;
            }
            pts
        }
        None => {
            let chron = read("chronicle.json")?;
            let events = chron.get("events").and_then(|e| e.as_array()).or_else(|| chron.as_array())?;
            let circuit = events
                .iter()
                .filter(|e| e.get("event").and_then(|v| v.as_str()) == Some("circuit"))
                .find(|e| name.is_empty() || e.get("name").and_then(|v| v.as_str()) == Some(name))?;
            let pts: Vec<Vec2> = circuit.get("path")?.as_array()?.iter().filter_map(xz).collect();
            if pts.len() < 3 {
                return None;
            }
            crate::world::chronicle::geom::spline(&pts, true, 2.0)
        }
    };
    let n = line.len();
    let length = (0..n).map(|i| line[i].distance(line[(i + 1) % n])).sum();
    Some(Course {
        world: world.to_string(),
        title: crate::menu::world_title(world),
        blurb: wj.get("blurb").and_then(|b| b.as_str()).unwrap_or("").to_string(),
        crossings: crossings(&line),
        line,
        length,
        laps: info.race_laps,
        rivals: info.race_rivals,
    })
}

/// How many times a closed line crosses itself.
fn crossings(line: &[Vec2]) -> usize {
    // Every few points is enough to find a crossing and quick enough to do
    // on the menu's first frame.
    let pts: Vec<Vec2> = line.iter().step_by(3).copied().collect();
    let n = pts.len();
    let cross = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        let (r, s) = (b - a, d - c);
        let den = r.perp_dot(s);
        if den.abs() < 1e-6 {
            return false;
        }
        let t = (c - a).perp_dot(s) / den;
        let u = (c - a).perp_dot(r) / den;
        (0.0..1.0).contains(&t) && (0.0..1.0).contains(&u)
    };
    let mut count = 0;
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if cross(pts[i], pts[(i + 1) % n], pts[j], pts[(j + 1) % n]) {
                count += 1;
            }
        }
    }
    count
}

/// What a kart card shows.
pub struct Kart {
    pub title: &'static str,
    pub about: &'static str,
    /// Top speed (km/h), pick-up (drive force over mass), grip and weight
    /// (kg), read from the library.
    pub stats: [f32; 4],
}

const KART_ABOUT: [&str; 3] = [
    "Even all round: the kart the courses were first tuned for.",
    "Fastest on the straights and wins the shoves, but slow off the line and slower to turn in.",
    "Off the line and out of the bends first, with a lower top speed. Easily shoved.",
];
const STAT_NAMES: [&str; 4] = ["TOP SPEED", "PICK-UP", "GRIP", "WEIGHT"];

pub fn karts() -> &'static [Kart] {
    static K: std::sync::OnceLock<Vec<Kart>> = std::sync::OnceLock::new();
    K.get_or_init(|| {
        let cat = crate::vehicle::Catalogue::load(&crate::library::data_root());
        crate::launch::KARTS
            .iter()
            .enumerate()
            .map(|(i, (name, title))| {
                let s = cat.get(name);
                Kart {
                    title,
                    about: KART_ABOUT.get(i).copied().unwrap_or(""),
                    stats: [s.top_speed * 3.6, s.force / s.mass.max(1.0), s.grip, s.mass],
                }
            })
            .collect()
    })
}

/// How full each stat's bar is: against the other karts, so the small
/// differences between them read, from a third full for the least to full
/// for the most.
fn bars(karts: &[Kart]) -> Vec<[f32; 4]> {
    karts
        .iter()
        .map(|k| {
            let mut out = [0.0; 4];
            for (s, o) in out.iter_mut().enumerate() {
                let (lo, hi) = karts.iter().map(|k| k.stats[s]).fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
                *o = if hi - lo < 1e-4 { 0.66 } else { 0.33 + 0.67 * (k.stats[s] - lo) / (hi - lo) };
            }
            out
        })
        .collect()
}

/// The option a race offers by this id, and where it is in the full list
/// (`Choice::LaunchOpt` indexes that list).
fn opt(l: &Launch, id: &str) -> Option<(usize, Opt)> {
    crate::launch::options(&l.world, l.def()).into_iter().enumerate().find(|(_, o)| o.id == id)
}

/// Which kart is picked now; None where the course isn't raced in karts.
pub fn kart_at(l: &Launch) -> Option<usize> {
    let (_, o) = opt(l, "kart")?;
    Some(l.index(&o))
}

/// The race step's lines: the race, then the conditions, then GO.
const RACE_OPTS: [&str; 6] = ["rivals", "laps", "difficulty", "hour", "sky", "season"];

/// The course cards, then RACE and KART AND OPTIONS for the card picked
/// (`pick`), then the arrows either side of the row. Only the cards are
/// stepped through with the keys; the rest are for the mouse.
pub fn course_items(pick: usize) -> Vec<Item> {
    let mut out: Vec<Item> = courses().iter().enumerate().map(|(i, c)| Item::new(&c.title, Choice::RaceCourse(i as u8))).collect();
    let p = pick as u8;
    out.push(Item::button("RACE", Choice::RaceCourse(p)));
    out.push(Item::button("KART AND OPTIONS", Choice::RaceOptions(p)));
    out.push(Item::new("", Choice::RaceShift(false)));
    out.push(Item::new("", Choice::RaceShift(true)));
    out
}

pub fn kart_items() -> Vec<Item> {
    karts().iter().enumerate().map(|(i, k)| Item::new(k.title, Choice::RaceKart(i as u8))).collect()
}

pub fn race_items(l: &Launch) -> Vec<Item> {
    let all = crate::launch::options(&l.world, l.def());
    let mut out: Vec<Item> = RACE_OPTS
        .iter()
        .filter_map(|id| all.iter().position(|o| o.id == *id))
        .map(|i| {
            let o = &all[i];
            Item {
                help: o.help,
                changed: l.changed(o),
                sub: l.under(o),
                ..Item::value(&o.label, Choice::LaunchOpt(i as u8), l.value(o))
            }
        })
        .collect();
    out.push(Item {
        help: "Onto the grid.",
        ..Item::button("GO", Choice::Start)
    });
    out.push(Item::button("BACK", Choice::Back));
    out
}

/// The race on one line, under the options: "Peregrine, heavy kart, 3 laps,
/// 5 rivals, hard, night, snow".
pub fn summary(l: &Launch) -> String {
    let v = |id: &str| opt(l, id).map(|(_, o)| l.value(&o));
    let info = crate::launch::info(&l.world);
    let mut parts = vec![soften(&crate::menu::world_title(&l.world))];
    if let Some(k) = v("kart") {
        parts.push(k.to_lowercase());
    }
    let laps = v("laps").and_then(|s| s.parse::<u32>().ok()).unwrap_or(info.race_laps);
    parts.push(format!("{laps} lap{}", if laps == 1 { "" } else { "s" }));
    let rivals = v("rivals").and_then(|s| s.parse::<u32>().ok()).unwrap_or(info.race_rivals);
    parts.push(format!("{rivals} rival{}", if rivals == 1 { "" } else { "s" }));
    // The lines left on their default say nothing, but for the difficulty,
    // which always has a level.
    for id in ["difficulty", "hour", "sky", "season"] {
        if let Some(s) = opt(l, id).filter(|(_, o)| l.under(o).is_none()).map(|(_, o)| l.value(&o)) {
            parts.push(s.to_lowercase());
        }
    }
    parts.join(", ")
}

/// The steps, top right: this one lit and underlined, the others quiet. Each
/// can be clicked to go to it, so their boxes go in `menu.tabs`.
fn steps(hud: &mut Hud, menu: &mut Menu, step: usize, w: f32, h: f32) {
    let u = unit(w, h);
    let p = pal();
    let st = Type::new(Face::Caps, 1.3 * u, 0.2);
    let gap = 3.0 * u;
    let widths: Vec<f32> = STEPS.iter().map(|s| Hud::measure(st, s)).collect();
    let total = widths.iter().sum::<f32>() + gap * (STEPS.len() - 1) as f32;
    let mut x = w - 7.0 * u - total;
    let base = 8.0 * u + st.cap();
    menu.tabs.clear();
    for (i, (s, sw)) in STEPS.iter().zip(&widths).enumerate() {
        let ink = if i == step { Ink::Grad(&p.lit) } else { Ink::Flat(with_a(p.ink, 0.7)) };
        hud.set_lit(st, x, base, ink, 0.6, s);
        if i == step {
            hud.rect(x, base + 1.0 * u, *sw, (0.12 * u).max(1.0), with_a(p.apricot, 0.9));
        }
        menu.tabs.push([x - gap * 0.5, base - st.cap() - 1.0 * u, sw + gap, st.cap() + 2.6 * u]);
        x += sw + gap;
    }
}

/// The screen's shade, its heading, the steps along the top, and the foot.
fn frame(hud: &mut Hud, menu: &mut Menu, heading: &str, step: usize, foot: &str, w: f32, h: f32) {
    let u = unit(w, h);
    let p = pal();
    hud.rect(0.0, 0.0, w, h, with_a(p.plum, 0.55));
    hud.grad_v(0.0, 0.0, w, 22.0 * u, with_a(p.plum, 0.5), with_a(p.plum, 0.0));
    let ht = heading_type(u);
    hud.set_lit(ht, 7.0 * u, 8.0 * u + ht.cap(), Ink::Grad(&p.lit), 0.8, heading);
    steps(hud, menu, step, w, h);
    if !foot.is_empty() {
        let nt = italic(u, 1.7);
        let text = soften(foot);
        let fw = Hud::measure(nt, &text);
        hud.set_lit(nt, ((w - fw) * 0.5).floor(), h - 3.6 * u, Ink::Flat(p.foot), 0.7, &text);
    }
}

/// A card's box: plum, its edge lit when it is the one picked.
fn card_box(hud: &mut Hud, r: [f32; 4], lit: bool, u: f32) {
    let p = pal();
    let [x, y, cw, ch] = r;
    hud.rect(x, y, cw, ch, with_a(p.plum, if lit { 0.88 } else { 0.66 }));
    let line = (if lit { 0.2 } else { 0.1 } * u).max(1.0);
    border(hud, r, line, if lit { with_a(p.apricot, 0.95) } else { with_a(p.ink, 0.28) });
    if lit {
        hud.grad_v(x, y, cw, 6.0 * u, with_a(p.rose, 0.22), with_a(p.rose, 0.0));
    }
}

/// Where the cards stand: up to `fit` of them in a row across the middle,
/// the picked one always among them. Returns the first shown and each shown
/// card's rectangle.
fn row(n: usize, at: usize, fit: usize, card_w: f32, card_h: f32, top: f32, w: f32, u: f32) -> (usize, Vec<[f32; 4]>) {
    let shown = n.min(fit).max(1);
    let first = at.saturating_sub(shown - 1).min(n.saturating_sub(shown)).min(at);
    let gap = 3.0 * u;
    let total = card_w * shown as f32 + gap * (shown - 1) as f32;
    let x0 = ((w - total) * 0.5).floor();
    (first, (0..shown).map(|k| [(x0 + k as f32 * (card_w + gap)).floor(), top, card_w, card_h]).collect())
}

/// Hit boxes for `n` cards, the shown ones in `rects` from `first`.
fn hits(menu: &mut Menu, n: usize, first: usize, rects: &[[f32; 4]]) {
    menu.hits = (0..n).map(|i| if i >= first && i < first + rects.len() { rects[i - first] } else { [0.0; 4] }).collect();
    menu.minus.clear();
    menu.plus.clear();
    menu.drops.clear();
    menu.track = [0.0; 4];
    menu.thumb = [0.0; 4];
}

/// Lines of `text` wrapped to `width`, set down from `y`, at most `most`.
#[allow(clippy::too_many_arguments)]
fn para(hud: &mut Hud, t: Type, x: f32, y: f32, width: f32, pitch: f32, most: usize, ink: [f32; 4], text: &str) -> f32 {
    let mut y = y;
    let lines = Hud::wrap(t, text, width);
    for (k, line) in lines.iter().take(most).enumerate() {
        // Cut short: the last line shown says there was more.
        let cut = k + 1 == most && lines.len() > most;
        let shown = if cut { format!("{}...", line.trim_end_matches(|c: char| c == ',' || c == ':' || c == ' ')) } else { line.clone() };
        hud.set_lit(t, x, y, Ink::Flat(ink), 0.6, &shown);
        y += pitch;
    }
    y
}

/// One course's card in `r`: its name, the circuit drawn (or with `pic` the
/// page image, the track from overhead), the facts, the player's best times
/// on it, and what the course is.
#[allow(clippy::too_many_arguments)]
fn course_card(hud: &mut Hud, c: &Course, best: Option<&crate::race::records::Best>, laps: u32, pic: bool, r: [f32; 4], lit: bool, u: f32) {
    let p = pal();
    card_box(hud, r, lit, u);
    let [x, y, cw, ch] = r;
    let pad = 2.4 * u;
    let (ix, iw) = (x + pad, cw - pad * 2.0);
    let tt = line_type(u, lit);
    let mut by = y + pad + tt.cap();
    hud.set_lit(tt, ix, by, if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink_value) }, 0.8, &c.title);
    by += 2.0 * u;
    // The track from overhead, a square as wide as the card or as the
    // room under the title and over the facts and bests allows.
    if pic {
        let side = iw.min(ch - (by - y) - 14.0 * u).max(0.0);
        hud.page((ix + (iw - side) * 0.5).round(), by.round(), side.round(), side.round());
        by += side + 2.2 * u;
    } else {
        by = circuit_drawn(hud, c, ch, ix, iw, by, lit, u);
    }
    card_rest(hud, c, best, laps, r, by, u);
}

/// The circuit, north up, in a box as wide as the card and a little
/// shorter, the start line marked; returns where the card goes on under it.
#[allow(clippy::too_many_arguments)]
fn circuit_drawn(hud: &mut Hud, c: &Course, ch: f32, ix: f32, iw: f32, by: f32, lit: bool, u: f32) -> f32 {
    let p = pal();
    let map_h = (ch * 0.32).min(iw * 0.7);
    let (lo, hi) = c.line.iter().fold((Vec2::splat(f32::MAX), Vec2::splat(f32::MIN)), |(lo, hi), q| (lo.min(*q), hi.max(*q)));
    let span = (hi - lo).max(Vec2::splat(1.0));
    let k = (iw / span.x).min(map_h / span.y) * 0.92;
    let mid = (lo + hi) * 0.5;
    let centre = Vec2::new(ix + iw * 0.5, by + map_h * 0.5);
    let to = |q: Vec2| centre + (q - mid) * k;
    let dot = (0.32 * u).max(2.0);
    let ink = if lit { with_a(p.ink_value, 0.95) } else { with_a(p.ink, 0.7) };
    for q in &c.line {
        let s = to(*q);
        hud.rect((s.x - dot * 0.5).round(), (s.y - dot * 0.5).round(), dot, dot, ink);
    }
    let start = to(c.line[0]);
    hud.star(start.x, start.y, 1.6 * u, p.apricot);
    by + map_h + 2.2 * u
}

/// What follows the circuit on a course card.
fn card_rest(hud: &mut Hud, c: &Course, best: Option<&crate::race::records::Best>, laps: u32, r: [f32; 4], mut by: f32, u: f32) {
    let p = pal();
    let [_, y, cw, ch] = r;
    let pad = 2.4 * u;
    let (ix, iw) = (r[0] + pad, cw - pad * 2.0);
    // The facts.
    let ft = italic(u, 1.55);
    let mut facts = format!("{:.0} m round, {} laps, {} rivals", c.length, c.laps, c.rivals);
    if c.crossings > 0 {
        facts.push_str(&format!(", {} crossing{}", c.crossings, if c.crossings == 1 { "" } else { "s" }));
    }
    by = para(hud, ft, ix, by, iw, 2.0 * u, 2, p.note, &facts) + 0.6 * u;
    // The bests, along the foot of the card.
    let rt = Type::new(Face::Caps, 1.2 * u, 0.15);
    let kart_word = |k: &str| crate::launch::KARTS.iter().find(|(n, _)| *n == k).map_or(k.to_string(), |(_, t)| t.to_lowercase());
    let mut lines: Vec<(String, String)> = Vec::new();
    if let Some(b) = best {
        if let Some(t) = b.race.get(&laps.to_string()) {
            lines.push((format!("BEST {laps}-LAP RACE"), format!("{}  {}", clock(t.secs, 2), kart_word(&t.kart))));
        }
        if let Some(t) = &b.lap {
            lines.push(("BEST LAP".to_string(), format!("{}  {}", clock(t.secs, 2), kart_word(&t.kart))));
        }
    }
    let pitch = 2.4 * u;
    let foot = y + ch - pad - pitch * lines.len().max(1) as f32;
    // A hairline over them.
    hud.rect(ix, (foot - 1.0 * u).round(), iw, (0.08 * u).max(1.0), with_a(p.ink, 0.22));
    if lines.is_empty() {
        hud.set_lit(rt, ix, foot + 0.6 * u + rt.cap(), Ink::Flat(with_a(p.ink, 0.5)), 0.5, "NO TIMES YET");
    } else {
        let vt = italic(u, 1.5);
        for (k, (l, v)) in lines.iter().enumerate() {
            let base = foot + 0.6 * u + rt.cap() + k as f32 * pitch;
            hud.set_lit(rt, ix, base, Ink::Flat(with_a(p.apricot, 0.9)), 0.6, l);
            let vw = Hud::measure(vt, v);
            hud.set_lit(vt, ix + iw - vw, base, Ink::Flat(p.ink_value), 0.6, v);
        }
    }
    // What the course is, in the room between the facts and the bests.
    let bt = italic(u, 1.4);
    let room = ((foot - 1.6 * u - by) / (1.85 * u)).floor().max(0.0) as usize;
    para(hud, bt, ix, by + 0.8 * u, iw, 1.85 * u, room, with_a(p.foot, 0.85), &c.blurb);
}

/// Step one: the courses, the picked one (`pick`) lit. Under the row, RACE
/// starts it with what was last picked for it (`summary`, in the foot) and
/// KART AND OPTIONS goes on to the other two steps.
#[allow(clippy::too_many_arguments)]
pub fn draw_courses(hud: &mut Hud, menu: &mut Menu, records: &Records, laps: Option<u32>, pick: usize, summary: &str, w: f32, h: f32) {
    let u = unit(w, h);
    let cs = courses();
    let n = cs.len();
    let foot = format!("{summary}. Enter to race, Tab for the kart and options");
    frame(hud, menu, "CHOOSE A COURSE", 0, &foot, w, h);
    let at = pick.min(n.saturating_sub(1));
    // The buttons sit on the bottom of the screen, as on the menus; the
    // cards take what is left above them.
    let bt = button_type(u);
    let bh = button_height(u);
    let by = (h - 6.5 * u - bh).floor();
    let top = 13.0 * u;
    let (cw, ch) = (30.0 * u, (by - 2.5 * u - top).min(58.0 * u));
    let fit = ((w - 16.0 * u) / (cw + 3.0 * u)).floor().max(1.0) as usize;
    let (first, rects) = row(n, at, fit.min(3), cw, ch, top, w, u);
    for (k, r) in rects.iter().enumerate() {
        let c = &cs[first + k];
        course_card(hud, c, records.best(&c.world), laps.unwrap_or(c.laps), false, *r, first + k == at, u);
    }
    hits(menu, n, first, &rects);
    // RACE and KART AND OPTIONS, centred under the row.
    let labels = ["RACE", "KART AND OPTIONS"];
    let widths: Vec<f32> = labels.iter().map(|l| (Hud::measure(bt, l) + 6.4 * u).max(16.0 * u)).collect();
    let gap = 2.2 * u;
    let mut bx = ((w - widths.iter().sum::<f32>() - gap) * 0.5).floor();
    for (k, (l, bw)) in labels.iter().zip(&widths).enumerate() {
        let r = [bx, by, *bw, bh];
        button(hud, r, l, menu.at == n + k, u);
        menu.hits.push(r);
        bx += bw + gap;
    }
    more_marks(hud, menu, first, rects.len(), n, &rects, u);
}

/// Arrows either side of the row when there are cards off it, each with how
/// many: clicking one moves the pick that way.
fn more_marks(hud: &mut Hud, menu: &mut Menu, first: usize, shown: usize, n: usize, rects: &[[f32; 4]], u: f32) {
    let p = pal();
    let at = menu.at;
    let (Some(a), Some(b)) = (rects.first(), rects.last()) else {
        menu.hits.extend([[0.0; 4]; 2]);
        return;
    };
    let mid = a[1] + a[3] * 0.5;
    let gt = italic(u, 5.0);
    let ct = Type::new(Face::Caps, 1.15 * u, 0.15);
    let mark = |hud: &mut Hud, x: f32, glyph: &str, more: usize, lit: bool| -> [f32; 4] {
        let ink = with_a(p.apricot, if lit { 1.0 } else { 0.8 });
        let gw = Hud::measure(gt, glyph);
        hud.set_lit(gt, (x - gw * 0.5).round(), (mid + gt.cap() * 0.5).round(), Ink::Flat(ink), 0.8, glyph);
        let words = format!("{more} MORE");
        let ww = Hud::measure(ct, &words);
        hud.set_lit(ct, (x - ww * 0.5).round(), (mid + gt.cap() * 0.5 + 2.4 * u).round(), Ink::Flat(with_a(p.ink, if lit { 0.9 } else { 0.6 })), 0.5, &words);
        [x - 3.5 * u, mid - 5.0 * u, 7.0 * u, 10.0 * u]
    };
    let left = if first > 0 { mark(hud, a[0] - 3.5 * u, "\u{2039}", first, at == n + 2) } else { [0.0; 4] };
    let right = if first + shown < n { mark(hud, b[0] + b[2] + 3.5 * u, "\u{203a}", n - first - shown, at == n + 3) } else { [0.0; 4] };
    menu.hits.push(left);
    menu.hits.push(right);
}

/// Step two: the karts, each with bars for what it is good at.
pub fn draw_karts(hud: &mut Hud, menu: &mut Menu, w: f32, h: f32) {
    let u = unit(w, h);
    let p = pal();
    let ks = karts();
    frame(hud, menu, "CHOOSE A KART", 1, "Left and right to choose, Enter to drive it, Esc for the courses", w, h);
    let at = menu.at.min(ks.len().saturating_sub(1));
    let (cw, ch) = (26.0 * u, (h - 21.0 * u).min(54.0 * u));
    let (first, rects) = row(ks.len(), at, 3, cw, ch, 14.5 * u, w, u);
    let fill = bars(ks);
    let colour = crate::vehicle::draw::kart_colour(0);
    for (k, r) in rects.iter().enumerate() {
        let i = first + k;
        let lit = i == at;
        let kart = &ks[i];
        card_box(hud, *r, lit, u);
        let [x, y, cw, _] = *r;
        let pad = 2.4 * u;
        let (ix, iw) = (x + pad, cw - pad * 2.0);
        // The player's colour along the top, the name under it.
        hud.rect(ix, y + pad, iw, 0.8 * u, with_a(colour, if lit { 1.0 } else { 0.6 }));
        let tt = line_type(u, lit);
        let mut by = y + pad + 2.6 * u + tt.cap();
        hud.set_lit(tt, ix, by, if lit { Ink::Grad(&p.lit) } else { Ink::Flat(p.ink_value) }, 0.8, &kart.title.to_uppercase());
        by += 3.2 * u;
        let st = Type::new(Face::Caps, 1.15 * u, 0.15);
        let vt = italic(u, 1.35);
        for s in 0..4 {
            hud.set_lit(st, ix, by + st.cap(), Ink::Flat(with_a(p.ink, 0.8)), 0.5, STAT_NAMES[s]);
            let v = match s {
                0 => format!("{:.0} km/h", kart.stats[0]),
                3 => format!("{:.0} kg", kart.stats[3]),
                _ => String::new(),
            };
            if !v.is_empty() {
                let vw = Hud::measure(vt, &v);
                hud.set_lit(vt, ix + iw - vw, by + st.cap(), Ink::Flat(p.note), 0.5, &v);
            }
            by += st.cap() + 1.0 * u;
            let bh = (0.7 * u).max(3.0);
            hud.rect(ix, by, iw, bh, with_a(p.ink, 0.14));
            hud.grad_h(ix, by, iw * fill[i][s], bh, with_a(p.coral, 0.95), with_a(p.mauve, 0.95));
            by += bh + 1.5 * u;
        }
        by += 0.6 * u;
        let at_ = italic(u, 1.45);
        let room = ((r[1] + r[3] - pad - by) / (1.9 * u)).floor().max(0.0) as usize;
        para(hud, at_, ix, by + at_.size * 0.6, iw, 1.9 * u, room, with_a(p.foot, 0.9), kart.about);
    }
    hits(menu, ks.len(), first, &rects);
}

/// A race world's picture of its track from overhead, if it has one
/// (`<world folder>/track.png`, a square shot from above the circuit).
pub fn track_picture(world: &str) -> Option<std::path::PathBuf> {
    Some(crate::worldload::world_dir(world).join("track.png")).filter(|p| p.is_file())
}

/// Step three: the race's options down the left, as the launch screen's
/// list, and the course picked on a card to the right, with the track from
/// overhead in it when `pic` (the page image holds `track_picture`).
pub fn draw_race(hud: &mut Hud, menu: &mut Menu, l: &Launch, records: &Records, pic: bool, w: f32, h: f32) {
    let u = unit(w, h);
    let items = race_items(l);
    let help = items.get(menu.at).map(|i| i.help).filter(|h| !h.is_empty()).unwrap_or("Left and right to change, Enter to go");
    let foot = format!("{help}\n{}", summary(l));
    crate::menu::draw_menu(hud, menu, "RACE", &items, &foot, crate::menu::Place::Left, w, h);
    // The steps line, over the list's own heading row.
    steps(hud, menu, 2, w, h);
    if let Some(c) = courses().iter().find(|c| c.world == l.world) {
        let (cw, ch) = (30.0 * u, (h - 30.0 * u).min(56.0 * u));
        let laps = opt(l, "laps").and_then(|(_, o)| l.value(&o).parse().ok()).unwrap_or(c.laps);
        if w > 58.0 * u + cw {
            // With the track's picture the card runs down to the foot.
            let ch = if pic { (h - 19.0 * u).max(ch) } else { ch };
            course_card(hud, c, records.best(&c.world), laps, pic, [w - 7.0 * u - cw, 16.0 * u, cw, ch], false, u);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A figure-eight crosses itself once; a ring never.
    #[test]
    fn crossings_are_counted() {
        let ring: Vec<Vec2> = (0..200).map(|i| Vec2::from_angle(i as f32 * std::f32::consts::TAU / 200.0) * 50.0).collect();
        assert_eq!(crossings(&ring), 0);
        let eight: Vec<Vec2> = (0..400)
            .map(|i| {
                let t = i as f32 * std::f32::consts::TAU / 400.0;
                Vec2::new(t.sin() * 60.0, (2.0 * t).sin() * 30.0)
            })
            .collect();
        assert_eq!(crossings(&eight), 1);
    }

    /// The race's worlds come back as course cards with a circuit drawn.
    #[test]
    fn every_race_world_has_a_card() {
        let cs = courses();
        assert!(cs.iter().any(|c| c.world == "peregrine"), "{:?}", cs.iter().map(|c| &c.world).collect::<Vec<_>>());
        for c in cs {
            assert!(c.length > 300.0 && c.line.len() > 100, "{}: {} m", c.world, c.length);
        }
        let k = cs.iter().find(|c| c.world == "vermilion").unwrap();
        assert_eq!(k.crossings, 1);
    }

    /// The bars set the karts apart: the heavy kart the heaviest and the
    /// quick one the quickest off the line.
    #[test]
    fn the_kart_bars_follow_the_specs() {
        let ks = karts();
        let b = bars(ks);
        // The cards stand in the order of `launch::KARTS`.
        let at = |n: &str| crate::launch::KARTS.iter().position(|(name, _)| *name == n).unwrap();
        assert!(b[at("kart_heavy")][3] > b[at("kart")][3] && b[at("kart")][3] > b[at("kart_quick")][3]);
        assert!(b[at("kart_quick")][1] > b[at("kart_heavy")][1]);
    }
}
