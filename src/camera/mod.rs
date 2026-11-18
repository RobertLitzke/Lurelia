//! The camera: the game's photography. F takes it out; a click raises the
//! viewfinder (the rest of the screen greys and blurs), a right click lowers
//! it again, Tab turns the frame (landscape, portrait, square), the wheel
//! zooms (Shift: the aperture), and a click takes the photo. Focus follows
//! whatever is under the centre box; Ctrl and the wheel, or holding Q or E,
//! focus by hand until a middle click or the next time the viewfinder comes
//! up. A roll holds 12; every
//! three finds earn a spare roll, and R loads it when the roll is finished.
//!
//! Each photo is checked against the five tasks (tasks.rs). One that does a
//! task goes into the scrapbook with its ribbon and reward; any other can be
//! kept as a snapshot (Enter) or let go. J opens the logbook (the tasks) and,
//! a page on, the scrapbook.
//!
//! The camera is kept by Game rather than as a modes::Activity: its film,
//! tasks and scrapbook outlive taking it out and putting it away, and the
//! world keeps running while it is up.

pub mod lens;
pub mod scene;
pub mod scrapbook;
pub mod tasks;

use crate::render::hud::Hud;
use lens::{Lens, Orient};
use scene::Shot;
use scrapbook::{Entry, Scrapbook};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use tasks::{Pool, Task};

pub const ROLL: u8 = 12;
/// Finds per new roll of film.
pub const FINDS_PER_ROLL: usize = 3;
const RELOAD_SECS: f32 = 1.6;

/// What progress.json keeps of the camera.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CameraSave {
    pub film: u8,
    pub rolls: u32,
    /// Finds already turned into rolls.
    pub credited: usize,
    pub tasks: Vec<Task>,
    pub next_task: u32,
    pub points: u32,
    pub orient: Orient,
    pub lens: Lens,
}

impl Default for CameraSave {
    fn default() -> Self {
        CameraSave {
            film: ROLL,
            rolls: 1,
            credited: 0,
            tasks: Vec::new(),
            next_task: 0,
            points: 0,
            orient: Orient::Landscape,
            lens: Lens::default(),
        }
    }
}

/// A photo waiting for the player to keep it or let it go.
pub struct Pending {
    pub px: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub entry: Entry,
}

/// What a photo did, for the logbook.
#[derive(Clone)]
pub struct LogLine {
    pub text: String,
    pub good: bool,
}

pub struct Camera {
    /// Out and in hand (P).
    pub active: bool,
    /// Looking through the viewfinder.
    pub raised: bool,
    pub orient: Orient,
    pub lens: Lens,
    pub film: u8,
    pub rolls: u32,
    credited: usize,
    /// Seconds left of loading a roll.
    pub reloading: f32,
    pub tasks: Vec<Task>,
    next_task: u32,
    pub points: u32,
    pool: Pool,
    seed: u32,
    /// The logbook's open page: 0 the tasks, 1.. the scrapbook.
    pub book: Option<usize>,
    /// The shutter fired this frame; the window or headless code develops it.
    pub shutter: bool,
    /// What the shutter saw, judged when it fired.
    pub judged: Option<(Shot, Option<usize>, Vec<String>)>,
    pub pending: Option<Pending>,
    pub flash: f32,
    pub log: VecDeque<LogLine>,
    pub scrapbook: Scrapbook,
    /// The page image needs drawing again.
    pub page_dirty: bool,
    pub page_labels: Vec<scrapbook::Label>,
    /// Distance to whatever is under the centre of the frame, for focusing.
    pub centre_m: Option<f32>,
    /// Left and right held last tick, to turn pages on a press.
    lr: (bool, bool),
    /// Focus set by hand (Q/E); otherwise autofocus on the centre.
    pub manual: bool,
}

impl Camera {
    pub fn new(save: Option<CameraSave>, finds: usize, pool: Pool, seed: u32, scrapbook: Scrapbook) -> Camera {
        let fresh = save.is_none();
        let s = save.unwrap_or_default();
        let mut c = Camera {
            active: false,
            raised: false,
            orient: s.orient,
            lens: s.lens,
            film: s.film.min(ROLL),
            rolls: s.rolls,
            // A save from before the camera: finds already made don't count.
            credited: if fresh { finds } else { s.credited },
            reloading: 0.0,
            tasks: s.tasks,
            next_task: s.next_task,
            points: s.points,
            pool,
            seed,
            book: None,
            shutter: false,
            judged: None,
            pending: None,
            flash: 0.0,
            log: VecDeque::new(),
            scrapbook,
            page_dirty: false,
            page_labels: Vec::new(),
            centre_m: None,
            lr: (false, false),
            manual: false,
        };
        c.fill_tasks();
        c
    }

    pub fn save(&self) -> CameraSave {
        CameraSave {
            film: self.film,
            rolls: self.rolls,
            credited: self.credited,
            tasks: self.tasks.clone(),
            next_task: self.next_task,
            points: self.points,
            orient: self.orient,
            lens: self.lens,
        }
    }

    fn fill_tasks(&mut self) {
        while self.tasks.len() < tasks::QUEUE {
            let t = tasks::make(self.seed, self.next_task, &self.pool);
            self.next_task += 1;
            self.tasks.push(t);
        }
    }

    /// Field of view as a multiple of `normal` while the viewfinder is up.
    pub fn zoom(&self, normal: f32) -> f32 {
        self.lens.fov() / normal
    }

    /// Turn finds into rolls of film; a note when one is earned.
    pub fn credit_finds(&mut self, finds: usize) -> Option<String> {
        let mut got = 0;
        while finds >= self.credited + FINDS_PER_ROLL {
            self.credited += FINDS_PER_ROLL;
            self.rolls += 1;
            got += 1;
        }
        (got > 0).then(|| format!("A NEW ROLL OF FILM  ({} SPARE)", self.rolls))
    }

    /// Put it away (P, or another mode taking over).
    pub fn put_away(&mut self) {
        self.active = false;
        self.raised = false;
    }

    /// One frame with the camera out. Takes the clicks and the wheel; returns
    /// a note for the player, if any.
    pub fn tick(&mut self, input: &crate::player::MoveInput, actions: &mut crate::game::Actions, dt: f32) -> Option<String> {
        self.flash = (self.flash - dt).max(0.0);
        let mut note = None;
        if self.reloading > 0.0 {
            self.reloading -= dt;
            if self.reloading <= 0.0 {
                self.reloading = 0.0;
                self.film = ROLL;
                note = Some(format!("LOADED: {ROLL} FRAMES  ({} SPARE)", self.rolls));
            }
        }
        if std::mem::take(&mut actions.reload) {
            note = Some(self.reload());
        }
        let click = std::mem::take(&mut actions.dig);
        let right = std::mem::take(&mut actions.place);
        let wheel = std::mem::take(&mut actions.brush_delta);
        let auto = std::mem::take(&mut actions.autofocus);
        let frame = std::mem::take(&mut actions.frame);
        if frame {
            self.orient = self.orient.next();
            note = Some(format!("{} FRAME", self.orient.name()));
        }
        if !self.raised {
            if click {
                self.raised = true;
                self.manual = false;
            }
            return note;
        }
        if right {
            // The viewfinder comes down; the camera stays in hand.
            self.raised = false;
            return note;
        }
        if wheel != 0 {
            if input.down {
                self.manual = true;
                self.lens.focus_by(wheel);
            } else if input.sprint {
                self.lens.stop_by(-wheel);
            } else {
                self.lens.zoom_by(wheel);
            }
        }
        if auto {
            self.manual = false;
        }
        let hand = input.focus_far as i32 as f32 - input.focus_near as i32 as f32;
        if hand != 0.0 {
            self.manual = true;
            self.lens.focus_hold(hand, dt);
        } else if !self.manual {
            // Nothing under the centre is the sky: focus far.
            self.lens.focus_toward(self.centre_m.unwrap_or(lens::FOCUS_MAX), dt);
        }
        if click {
            note = self.fire();
        }
        note
    }

    fn reload(&mut self) -> String {
        if self.reloading > 0.0 {
            "LOADING FILM".into()
        } else if self.film > 0 {
            format!("{} FRAMES LEFT ON THIS ROLL", self.film)
        } else if self.rolls == 0 {
            format!("NO SPARE FILM: EVERY {FINDS_PER_ROLL} FINDS EARN A ROLL")
        } else {
            self.rolls -= 1;
            self.reloading = RELOAD_SECS;
            self.raised = false;
            "LOADING FILM".into()
        }
    }

    /// Press the shutter.
    fn fire(&mut self) -> Option<String> {
        if self.reloading > 0.0 {
            return Some("LOADING FILM".into());
        }
        if self.film == 0 {
            return Some(if self.rolls > 0 {
                "END OF THE ROLL: R TO LOAD ANOTHER".into()
            } else {
                format!("OUT OF FILM: EVERY {FINDS_PER_ROLL} FINDS EARN A ROLL")
            });
        }
        self.film -= 1;
        self.shutter = true;
        self.flash = 0.2;
        // A photo waiting for a decision is let go by the next.
        self.pending = None;
        None
    }

    /// Judge what the shutter saw against the tasks: the best task done (the
    /// largest reward), or what the nearest miss lacked.
    pub fn judge(&mut self, shot: Shot) {
        let mut best: Option<usize> = None;
        let mut nearest: Option<Vec<String>> = None;
        for (i, t) in self.tasks.iter().enumerate() {
            match t.check(&shot) {
                Ok(()) => {
                    if best.is_none_or(|b| self.tasks[b].reward < t.reward) {
                        best = Some(i);
                    }
                }
                Err(miss) => {
                    // Nearest: a task whose subject is in the photo, fewest misses.
                    let framed = t.subject().is_some_and(|n| shot.seen.iter().any(|s| s.is(n) && s.in_frame()));
                    if framed && nearest.as_ref().is_none_or(|m| miss.len() < m.len()) {
                        nearest = Some(miss.iter().map(|m| format!("{}: {m}", t.title)).collect());
                    }
                }
            }
        }
        self.judged = Some((shot, best, nearest.unwrap_or_default()));
    }

    /// The developed photo (RGBA) of the last shutter. A done task goes
    /// straight into the scrapbook; anything else waits for Enter. Returns the
    /// file writer, if one started, and a note.
    pub fn developed(&mut self, px: Vec<u8>, w: u32, h: u32) -> (Option<std::thread::JoinHandle<Result<std::path::PathBuf, String>>>, String) {
        let Some((shot, done, miss)) = self.judged.take() else {
            return (None, String::new());
        };
        let mut subjects: Vec<String> = shot
            .seen
            .iter()
            .filter(|s| s.in_frame() && s.visible && s.sharp && s.size >= tasks::MIN_SIZE)
            .map(|s| s.name.clone())
            .collect();
        subjects.sort();
        subjects.dedup();
        let entry = Entry {
            hour: shot.hour,
            subjects,
            ..Default::default()
        };
        match done {
            Some(i) => {
                let t = self.tasks.remove(i);
                self.points += t.reward;
                self.fill_tasks();
                let e = Entry {
                    task: Some(t.title.clone()),
                    needs: t.needs.iter().map(|n| n.line()).collect(),
                    reward: t.reward,
                    ..entry
                };
                let writer = self.scrapbook.add(e, px, w, h);
                self.page_dirty = true;
                let msg = format!("TASK DONE: {}  +{}", t.title, t.reward);
                self.push_log(&msg, true);
                (writer, msg)
            }
            None => {
                let why = miss.first().cloned().unwrap_or_else(|| "NO TASK IN THIS PHOTO".to_string());
                self.push_log(&why, false);
                self.pending = Some(Pending { px, w, h, entry });
                (None, why)
            }
        }
    }

    /// Enter: keep the waiting photo as a snapshot.
    pub fn keep(&mut self) -> Option<std::thread::JoinHandle<Result<std::path::PathBuf, String>>> {
        let p = self.pending.take()?;
        self.page_dirty = true;
        self.push_log("KEPT AS A SNAPSHOT", true);
        self.scrapbook.add(p.entry, p.px, p.w, p.h)
    }

    fn push_log(&mut self, text: &str, good: bool) {
        self.log.push_front(LogLine { text: text.to_string(), good });
        self.log.truncate(6);
    }

    /// Esc: let a waiting photo go, close the book, or lower the viewfinder.
    /// False when there was nothing to back out of.
    pub fn back(&mut self) -> bool {
        if self.pending.take().is_some() {
            true
        } else if self.book.take().is_some() {
            true
        } else if self.raised {
            self.raised = false;
            true
        } else {
            false
        }
    }

    pub fn wants_escape(&self) -> bool {
        self.pending.is_some() || self.book.is_some() || (self.active && self.raised)
    }

    /// J: open or close the logbook.
    pub fn toggle_book(&mut self) {
        self.book = match self.book {
            Some(_) => None,
            None => Some(0),
        };
        self.page_dirty = true;
    }

    /// The book is open: A and D (or the arrows) turn pages.
    pub fn book_tick(&mut self, input: &crate::player::MoveInput) {
        let Some(page) = self.book else { return };
        let last = self.scrapbook.pages();
        let (l, r) = (input.left, input.right);
        let mut p = page;
        if l && !self.lr.0 && p > 0 {
            p -= 1;
        }
        if r && !self.lr.1 && p < last {
            p += 1;
        }
        self.lr = (l, r);
        if p != page {
            self.book = Some(p);
            self.page_dirty = true;
        }
    }

    /// A new page image when a scrapbook page is open and has changed.
    pub fn page_image(&mut self) -> Option<Vec<u8>> {
        let page = self.book?;
        if page == 0 || !self.page_dirty {
            return None;
        }
        self.page_dirty = false;
        let (img, labels) = self.scrapbook.draw_page(page - 1);
        self.page_labels = labels;
        Some(img)
    }

    /// The camera's HUD: the viewfinder when it is up, a status line when not.
    pub fn hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32) {
        let ink = [1.0, 0.95, 0.9, 0.9];
        let quiet = [1.0, 0.95, 0.9, 0.72];
        let line = 10.0 * s;
        if self.raised {
            let f = lens::frame_uv(self.orient, w / h);
            let (x0, y0, x1, y1) = ((f[0] * w).round(), (f[1] * h).round(), (f[2] * w).round(), (f[3] * h).round());
            let c = [1.0, 1.0, 1.0, 0.7];
            hud.rect(x0, y0, x1 - x0, 1.0, c);
            hud.rect(x0, y1 - 1.0, x1 - x0, 1.0, c);
            hud.rect(x0, y0, 1.0, y1 - y0, c);
            hud.rect(x1 - 1.0, y0, 1.0, y1 - y0, c);
            // Corner brackets.
            let (l, t) = (18.0 * s, 2.0);
            for (cx, cy, dx, dy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
                hud.rect(if dx > 0.0 { cx } else { cx - l }, if dy > 0.0 { cy } else { cy - t }, l, t, [1.0, 1.0, 1.0, 0.95]);
                hud.rect(if dx > 0.0 { cx } else { cx - t }, if dy > 0.0 { cy } else { cy - l }, t, l, [1.0, 1.0, 1.0, 0.95]);
            }
            // Faint thirds.
            for k in [1.0f32, 2.0] {
                hud.rect((x0 + (x1 - x0) * k / 3.0).floor(), y0, 1.0, y1 - y0, [1.0, 1.0, 1.0, 0.12]);
                hud.rect(x0, (y0 + (y1 - y0) * k / 3.0).floor(), x1 - x0, 1.0, [1.0, 1.0, 1.0, 0.12]);
            }
            // The focus point: green when what is under it is sharp.
            let (cx, cy) = ((w * 0.5).floor(), (h * 0.5).floor());
            let sharp = self.centre_m.is_some_and(|d| self.lens.sharp(d));
            let fc = if sharp { [0.45, 1.0, 0.5, 0.95] } else { [1.0, 1.0, 1.0, 0.7] };
            focus_box(hud, cx, cy, s, fc);
            // Settings under the frame.
            let centre = self.centre_m.map_or("--".to_string(), |d| format!("{d:.1} M"));
            let info = format!(
                "{}   {:.0} MM   F/{}   FOCUS {:.1} M {}   CENTRE {}   FILM {}/{}",
                self.orient.name(),
                self.lens.focal_mm,
                fnum(self.lens.f_number()),
                self.lens.focus_m,
                if self.manual { "MF" } else { "AF" },
                centre,
                self.film,
                ROLL
            );
            let iw = Hud::text_width(&info, s);
            hud.label((cx - iw * 0.5).floor(), (y1 + 6.0).min(h - line - 4.0), s, ink, &info);
            let keys = "CLICK SHUTTER   RIGHT CLICK LOWER   WHEEL ZOOM   CTRL+WHEEL OR Q E FOCUS   MIDDLE CLICK AUTOFOCUS   SHIFT+WHEEL APERTURE   TAB FRAME";
            let kw = Hud::text_width(keys, s);
            hud.label((cx - kw * 0.5).floor(), (y0 - line - 4.0).max(4.0), s, quiet, keys);
            if self.flash > 0.0 {
                hud.rect(x0, y0, x1 - x0, y1 - y0, [1.0, 1.0, 1.0, (self.flash * 3.0).min(0.6)]);
            }
        } else {
            let film = if self.reloading > 0.0 {
                "LOADING FILM".to_string()
            } else {
                format!("FILM {}/{}   SPARE ROLLS {}", self.film, ROLL, self.rolls)
            };
            let lines = [format!("CAMERA   {film}"), "CLICK VIEWFINDER   TAB FRAME   R LOAD FILM   J LOGBOOK   F PUT AWAY".to_string()];
            let y = h - line * 2.0 - 24.0;
            // A soft shade up from the bottom edge rather than a box.
            let band = h - y + 2.0 * line;
            hud.grad_v(0.0, h - band, w, band, [0.03, 0.015, 0.025, 0.0], [0.03, 0.015, 0.025, 0.4]);
            hud.label(20.0, y, s, ink, &lines[0]);
            hud.label(20.0, y + line, s, quiet, &lines[1]);
        }
        // The tasks, top right; small while framing a photo.
        let ts = if self.raised { (s * 0.5).max(1.0).round() } else { s };
        let tl = 10.0 * ts;
        let x = w - 20.0 - self.tasks.iter().map(|t| Hud::text_width(&t.title, ts)).fold(Hud::text_width("TASKS", ts), f32::max);
        let y0 = 16.0 + 24.0 * s;
        hud.label(x, y0, ts, quiet, "TASKS");
        for (i, t) in self.tasks.iter().enumerate() {
            hud.label(x, y0 + (i + 1) as f32 * tl, ts, ink, &t.title);
        }
        if self.pending.is_some() {
            let m = "ENTER  KEEP AS A SNAPSHOT     ESC  LET IT GO";
            let mw = Hud::text_width(m, s);
            hud.label(((w - mw) * 0.5).floor(), (h * 0.3 + 18.0 * s).floor(), s, ink, m);
        }
    }

    /// The logbook (page 0) or a scrapbook page, over a dimmed, paused game.
    pub fn book_hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32) {
        let Some(page) = self.book else { return };
        let ink = [1.0, 0.96, 0.86, 0.95];
        let amber = crate::menu::pal().apricot;
        let dim = [0.75, 0.72, 0.66, 0.9];
        hud.rect(0.0, 0.0, w, h, [0.0, 0.0, 0.0, 0.88]);
        let pages = self.scrapbook.pages();
        let foot = format!("PAGE {} OF {}    A / D TURN    J OR ESC CLOSE", page + 1, pages + 1);
        if page == 0 {
            let line = 11.0 * s;
            let colw = (w * 0.7).min(110.0 * 6.0 * s);
            let x0 = ((w - colw) * 0.5).floor();
            let mut y = (h * 0.08).floor();
            hud.label(x0, y, s * 2.0, ink, "LOGBOOK");
            let head = format!(
                "FILM {}/{}   SPARE ROLLS {}   REWARDS {}   PHOTOS KEPT {}",
                self.film,
                ROLL,
                self.rolls,
                self.points,
                self.scrapbook.entries.len()
            );
            hud.label(x0, y + 24.0 * s, s, amber, &head);
            y += 24.0 * s + line * 2.0;
            for (i, t) in self.tasks.iter().enumerate() {
                hud.label(x0, y, s, ink, &format!("{}. {}", i + 1, t.title));
                let rw = format!("+{}", t.reward);
                hud.label(x0 + colw - Hud::text_width(&rw, s), y, s, amber, &rw);
                y += line;
                for n in &t.needs {
                    hud.label(x0 + 24.0 * s, y, s, dim, &format!("- {}", n.line()));
                    y += line;
                }
                y += line * 0.5;
            }
            if !self.log.is_empty() {
                y += line * 0.5;
                hud.label(x0, y, s, amber, "LAST PHOTOS");
                y += line;
                for l in &self.log {
                    let c = if l.good { [0.6, 1.0, 0.6, 0.95] } else { dim };
                    hud.label(x0, y, s, c, &l.text);
                    y += line;
                }
            }
            let fw = Hud::text_width(&foot, s);
            hud.label(((w - fw) * 0.5).floor(), h - line * 2.0, s, dim, &foot);
            return;
        }
        let size = (h * 0.88).min(w * 0.9).floor();
        let (x0, y0) = (((w - size) * 0.5).floor(), ((h - size) * 0.5 - 6.0 * s).floor());
        hud.page(x0, y0, size, size);
        let k = size / crate::render::hud::IMAGE_PX as f32;
        // Captions in whole font pixels, cut short to their room on the page.
        for l in &self.page_labels {
            let ls = (s * l.scale).round().max(1.0);
            let room = (l.max_w * k / (6.0 * ls)).floor().max(1.0) as usize;
            let text: String = l.text.chars().take(room).collect();
            hud.text(x0 + l.x * k, y0 + l.y * k, ls, l.color, &text);
        }
        let title = format!("SCRAPBOOK  {}", page);
        hud.label(x0, (y0 - 12.0 * s).max(2.0), s, ink, &title);
        let fw = Hud::text_width(&foot, s);
        hud.label(((w - fw) * 0.5).floor(), y0 + size + 4.0, s, dim, &foot);
    }
}

fn fnum(n: f32) -> String {
    if n.fract().abs() < 0.01 {
        format!("{n:.0}")
    } else {
        format!("{n:.1}")
    }
}

/// The small square at the centre of the screen while the camera is out, and
/// the viewfinder's focus point (green when what is under it is sharp).
pub fn focus_box(hud: &mut Hud, cx: f32, cy: f32, s: f32, c: [f32; 4]) {
    let r = 6.0 * s;
    hud.rect(cx - r, cy - r, 2.0 * r, 1.5, c);
    hud.rect(cx - r, cy + r - 1.5, 2.0 * r, 1.5, c);
    hud.rect(cx - r, cy - r, 1.5, 2.0 * r, c);
    hud.rect(cx + r - 1.5, cy - r, 1.5, 2.0 * r, c);
}
