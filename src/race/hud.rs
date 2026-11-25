//! The race's HUD: place, laps and times, the standings, the minimap, the
//! countdown and the results.

use super::laps::AWAY_SECS;
use super::arms::{Item, Slot};
use super::fight::name;
use super::{Phase, Race, RUN_IN, WRONG_AFTER};
use crate::render::hud::Hud;
use crate::vehicle::bot::Track;
use crate::vehicle::draw::kart_colour;
use glam::Vec2;

/// Seconds a split stays on the HUD.
const SPLIT_SECS: f32 = 3.0;

/// The circuit drawn small: its centreline in a unit square, north up.
pub(super) struct Map {
    dots: Vec<Vec2>,
    centre: Vec2,
    span: f32,
}

impl Map {
    /// Dots enough to draw the line solid at any size the HUD uses.
    const DOTS: usize = 480;

    pub(super) fn new(track: &Track) -> Map {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for p in &track.points {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        let centre = (lo + hi) * 0.5;
        let span = (hi - lo).max_element().max(1.0) * 1.08;
        let mut m = Map { dots: Vec::new(), centre, span };
        m.dots = (0..Self::DOTS).map(|i| m.unit(track.at(track.length * i as f32 / Self::DOTS as f32).0)).collect();
        m
    }

    /// A point on the ground (x, z) in the unit square.
    pub(super) fn unit(&self, p: Vec2) -> Vec2 {
        (p - self.centre) / self.span + Vec2::splat(0.5)
    }
}

impl Race {
    /// The whole HUD (`Activity::hud`).
    pub(super) fn draw_hud(&self, hud: &mut Hud, w: f32, h: f32, s: f32) {
        let white = [1.0, 1.0, 1.0, 0.95];
        let dim = [0.85, 0.9, 0.95, 0.85];
        let warm = [1.0, 0.82, 0.35, 1.0];
        let centre = |hud: &mut Hud, y: f32, size: f32, c: [f32; 4], t: &str| {
            let tw = Hud::text_width(t, size);
            hud.label((w * 0.5 - tw * 0.5).floor(), y, size, c, t);
        };
        let n = self.bots.len() + 1;
        let edge = w - 24.0;
        // Place, top right: the ordinal large, out of how many beside it on
        // the same baseline.
        let (big, small) = (s * 3.2, s * 1.4);
        let of = format!("/{n}");
        let place = ordinal(self.position());
        let ow = Hud::text_width(&of, small);
        hud.label((edge - ow).floor(), 20.0 + 6.4 * (big - small), small, dim, &of);
        hud.label((edge - ow - Hud::text_width(&place, big) - 2.0 * s).floor(), 20.0, big, white, &place);
        let right = |hud: &mut Hud, y: f32, size: f32, c: [f32; 4], t: &str| {
            let tw = Hud::text_width(t, size);
            hud.label((edge - tw).floor(), y, size, c, t);
        };
        // The lap, the race clock, and the last and best laps.
        let lap = self.me.lap(self.track.length, self.laps);
        let mut y = 20.0 + 34.0 * s;
        if lap == self.laps && self.laps > 1 && self.me.finish.is_none() {
            right(hud, y, s * 1.4, warm, "FINAL LAP");
        } else {
            right(hud, y, s * 1.4, white, &format!("LAP {lap}/{}", self.laps));
        }
        y += 16.0 * s;
        right(hud, y, s * 1.3, white, &clock(self.me.finish.unwrap_or(self.clock), 1));
        y += 15.0 * s;
        if let (Some(last), Some(best)) = (self.times.last(), self.best_lap()) {
            let bright = [0.92, 0.95, 1.0, 0.92];
            right(hud, y, s * 1.1, bright, &format!("LAST {}", clock(*last, 1)));
            y += 12.0 * s;
            right(hud, y, s * 1.1, bright, &format!("BEST {}", clock(best, 1)));
            y += 12.0 * s;
        }
        // The last split or lap against the best, for a few seconds: green
        // quicker, red slower.
        if let Some((d, at)) = self.split {
            if self.clock - at < SPLIT_SECS {
                let (ink, sign) = if d <= 0.0 { ([0.45, 1.0, 0.5, 1.0], "-") } else { ([1.0, 0.42, 0.36, 1.0], "+") };
                right(hud, y, s * 1.3, ink, &format!("{sign}{:.2}", d.abs()));
                y += 15.0 * s;
            }
        }
        self.standings_panel(hud, edge, y + 6.0 * s, s, white, warm);
        self.minimap(hud, w, h, s);
        let red = [1.0, 0.36, 0.26, 1.0];
        if self.wrong > WRONG_AFTER && self.me.finish.is_none() {
            if self.clock.fract() < 0.65 {
                centre(hud, h * 0.38, s * 3.0, red, "WRONG WAY");
            }
            // Which way to turn round, the shorter way.
            let turn = if self.wrong_turn < 0.0 { "<<  TURN LEFT" } else { "TURN RIGHT  >>" };
            centre(hud, h * 0.38 + 34.0 * s, s * 1.4, white, turn);
        }
        // Off the road: a word on the verge, and far out of bounds the
        // countdown to being put back on the track.
        if let (Some(p), None) = (self.me.place, self.me.finish) {
            let half = self.track.width_at(p.s) * 0.5;
            if self.me.away > 0.0 {
                let left = (AWAY_SECS - self.me.away).ceil().max(1.0) as u32;
                centre(hud, h * 0.52, s * 2.0, red, &format!("RETURN TO TRACK  {left}"));
            } else if p.off.abs() > half + 1.0 && !matches!(self.phase, Phase::Grid(_)) {
                centre(hud, h * 0.58, s * 1.3, warm, "OFF TRACK");
            }
        }
        // Home, the player drives no more: only the results.
        let home = self.me.finish.is_some();
        if !home {
            self.slot_panel(hud, h, s, white, dim);
            // Speed, bottom centre, and the polish under the player when
            // there is any.
            centre(hud, h - 34.0 * s, s * 1.6, white, &format!("{:.0} KM/H", self.speed.abs() * 3.6));
        }
        if self.my_polish > 1.0005 && !home {
            centre(hud, h - 20.0 * s, s * 0.9, [0.8, 0.85, 0.95, 0.85], &format!("POLISH +{:.1}%", (self.my_polish - 1.0) * 100.0));
        }
        let pips = match self.tier {
            0 if self.boosting => "BOOST".to_string(),
            0 => String::new(),
            t => format!("DRIFT {}", "*".repeat(t as usize)),
        };
        if !home && !pips.is_empty() {
            centre(hud, h - 50.0 * s, s * 1.2, warm, &pips);
        } else if !home && self.drafting() {
            centre(hud, h - 50.0 * s, s * 1.2, [0.7, 0.9, 1.0, 0.95], "SLIPSTREAM");
        }
        match self.phase {
            Phase::Grid(t) => {
                let left = t.ceil() as u32;
                let say = if left > 3 { "READY".to_string() } else { left.to_string() };
                // Above where the gantry's lights stand from the grid.
                centre(hud, h * 0.16, s * 4.0, warm, &say);
                centre(hud, h * 0.16 + 44.0 * s, s, dim, "W DRIVE  A D STEER  SPACE HOP, HOLD TO DRIFT  LEFT MOUSE USES THE ITEM, RIGHT THROWS IT AHEAD  SHIFT BOOSTS");
                if let Some(c) = &self.cup {
                    centre(hud, h * 0.16 - 22.0 * s, s * 1.3, white, &format!("CUP  RACE {} OF {}", c.at + 1, c.courses.len()));
                }
            }
            Phase::Racing => {}
            Phase::Done(since) => {
                // The results, as they come in; in a cup, the points table
                // beside them.
                let y0 = h * 0.26;
                let (bw, bh) = (200.0 * s, (58.0 + 14.0 * n as f32) * s);
                let mid = if self.cup.is_some() { w * 0.5 - bw * 0.5 - 6.0 * s } else { w * 0.5 };
                let at = |hud: &mut Hud, x: f32, y: f32, size: f32, c: [f32; 4], t: &str| {
                    let tw = Hud::text_width(t, size);
                    hud.label((x - tw * 0.5).floor(), y, size, c, t);
                };
                hud.rect((mid - bw * 0.5).floor(), y0 - 8.0 * s, bw, bh, [0.0, 0.0, 0.0, 0.5]);
                at(hud, mid, y0, s * 1.8, white, "RESULTS");
                for (k, (who, _, fin)) in self.standings().iter().enumerate() {
                    let time = fin.map_or_else(|| if since > RUN_IN { "DNF".to_string() } else { "RACING".to_string() }, |t| clock(t, 1));
                    let c = if *who == 0 { warm } else { white };
                    at(hud, mid, y0 + (22.0 + 14.0 * k as f32) * s, s * 1.2, c, &format!("{}  {:<6}  {}", ordinal(k + 1), name(*who), time));
                }
                if let Some(best) = self.best_lap() {
                    at(hud, mid, y0 + (24.0 + 14.0 * n as f32) * s, s, dim, &format!("YOUR BEST LAP {}", clock(best, 1)));
                }
                let enter = match &self.cup {
                    None => "ENTER TO RACE AGAIN".to_string(),
                    Some(c) if c.last() => "ENTER FOR A NEW CUP".to_string(),
                    Some(c) => format!("ENTER FOR RACE {}: {}", c.at + 2, crate::menu::world_title(&c.courses[c.at + 1]).to_uppercase()),
                };
                centre(hud, y0 + bh + 2.0 * s, s, dim, &enter);
                if let Some(c) = &self.cup {
                    let x = w * 0.5 + bw * 0.5 + 6.0 * s;
                    hud.rect((x - bw * 0.5).floor(), y0 - 8.0 * s, bw, bh, [0.0, 0.0, 0.0, 0.5]);
                    let title = if c.last() && c.scored() { "CUP FINAL".to_string() } else { format!("CUP  RACE {}/{}", c.at + 1, c.courses.len()) };
                    at(hud, x, y0, s * 1.8, white, &title);
                    if !c.scored() {
                        at(hud, x, y0 + 22.0 * s, s * 1.1, dim, "POINTS WHEN ALL ARE HOME");
                    }
                    for (k, (who, pts)) in c.table().iter().enumerate().filter(|_| c.scored()) {
                        let ink = if *who == 0 { warm } else { white };
                        at(hud, x, y0 + (22.0 + 14.0 * k as f32) * s, s * 1.2, ink, &format!("{}  {:<6}  {pts:>2} PTS", ordinal(k + 1), name(*who)));
                    }
                }
            }
        }
    }

    /// What the player carries, bottom left: the item in its colour and the
    /// uses left, and the shield or
    /// overdrive while either is on. With `KART_ARMS=all`, how to pick.
    fn slot_panel(&self, hud: &mut Hud, h: f32, s: f32, white: [f32; 4], dim: [f32; 4]) {
        let (x0, y0) = (24.0, h - 62.0 * s);
        let (w, hh) = (96.0 * s, 30.0 * s);
        hud.rect(x0, y0, w, hh, [0.0, 0.0, 0.0, 0.35]);
        let (label, colour, left) = match self.slots.first().copied().unwrap_or(Slot::Empty) {
            Slot::Empty => ("NO ITEM".to_string(), [0.6, 0.6, 0.6, 0.6], None),
            Slot::Have(i, n) => (i.name().to_string(), i.colour(), (self.arms.rules.uses(i) > 1).then_some(n)),
        };
        hud.rect(x0, y0, 5.0 * s, hh, colour);
        hud.label(x0 + 10.0 * s, y0 + 6.0 * s, s * 1.5, white, &label);
        if let Some(n) = left {
            let t = format!("x{n}");
            hud.label(x0 + w - Hud::text_width(&t, s * 1.2) - 6.0 * s, y0 + 8.0 * s, s * 1.2, dim, &t);
        }
        let h0 = self.harm.first().copied().unwrap_or_default();
        // Damage, under the slot: a bar filling toward the most a kart can
        // lose, amber then red, and the speed it costs.
        let dy = y0 + hh + 4.0 * s;
        let dh = 5.0 * s;
        hud.rect(x0, dy, w, dh, [0.0, 0.0, 0.0, 0.35]);
        let share = h0.damage / self.arms.rules.damage_max;
        if share > 0.0 {
            let ink = if share > 0.66 { [1.0, 0.32, 0.25, 0.95] } else { [1.0, 0.72, 0.25, 0.95] };
            hud.rect(x0, dy, w * share, dh, ink);
        }
        hud.label(x0, dy + dh + 2.0 * s, s * 0.9, dim, &format!("DAMAGE {:.0}%", h0.damage * 100.0));
        let mut y = y0 - 14.0 * s;
        if h0.shield > 0.0 {
            hud.label(x0, y, s * 1.1, Item::Shield.colour(), &format!("SHIELD {:.0}", h0.shield.ceil()));
            y -= 12.0 * s;
        }
        if h0.over > 0.0 {
            hud.label(x0, y, s * 1.1, Item::Overdrive.colour(), &format!("OVERDRIVE {:.0}", h0.over.ceil()));
        }
        if self.arms.all {
            hud.label(x0, dy + dh + 13.0 * s, s * 0.9, dim, "1-5 PICK  LEFT MOUSE FIRES");
        }
    }

    /// The running order, under the lap times at the right edge: each car's
    /// place, colour and name, and how far it is behind the one in front,
    /// in seconds at the last checkpoint both passed (a finisher shows its
    /// time).
    fn standings_panel(&self, hud: &mut Hud, edge: f32, y0: f32, s: f32, white: [f32; 4], warm: [f32; 4]) {
        let rows = self.standings();
        let (size, line) = (s * 1.1, 13.0 * s);
        let pw = 96.0 * s;
        let x0 = edge - pw;
        hud.rect(x0 - 6.0 * s, y0 - 4.0 * s, pw + 6.0 * s, line * rows.len() as f32 + 6.0 * s, [0.0, 0.0, 0.0, 0.3]);
        for (k, &(who, _, fin)) in rows.iter().enumerate() {
            let y = y0 + k as f32 * line;
            let ink = if who == 0 { warm } else { white };
            let chip = 6.0 * s;
            hud.rect(x0, y + 1.0 * s, chip, chip, kart_colour(who));
            hud.label(x0 + chip + 4.0 * s, y, size, ink, &format!("{} {}", k + 1, name(who)));
            let gap = match (fin, k) {
                (Some(t), _) => clock(t, 1),
                (None, 0) => String::new(),
                (None, _) => self.check_gap(rows[k - 1].0, who).map_or(String::new(), |g| format!("+{:.1}", g.max(0.0))),
            };
            if !gap.is_empty() {
                let gw = Hud::text_width(&gap, size);
                hud.label((edge - gw).floor(), y, size, ink, &gap);
            }
        }
    }

    /// The circuit small in the bottom right corner, north up, with every
    /// car on it in its colour and the player's drawn largest, on top.
    fn minimap(&self, hud: &mut Hud, w: f32, h: f32, s: f32) {
        let m = (96.0 * s).min(h * 0.3).floor();
        let (x0, y0) = (w - m - 24.0, h - m - 24.0);
        let at = |u: Vec2| (x0 + u.x * m, y0 + u.y * m);
        let dot = |hud: &mut Hud, (x, y): (f32, f32), d: f32, c: [f32; 4]| hud.rect((x - d * 0.5).floor(), (y - d * 0.5).floor(), d, d, c);
        let road = (2.5 * s).max(3.0);
        for &u in &self.map.dots {
            dot(hud, at(u), road + 2.0 * s, [0.0, 0.0, 0.0, 0.55]);
        }
        for &u in &self.map.dots {
            dot(hud, at(u), road, [0.93, 0.93, 0.9, 0.9]);
        }
        // The checkpoints, as dark ticks on the line.
        let every = self.check_every;
        let mut c = every;
        while c < self.track.length - every * 0.5 {
            let (q, _) = self.track.at(c);
            dot(hud, at(self.map.unit(q)), road + 1.0 * s, [0.2, 0.2, 0.25, 0.9]);
            c += every;
        }
        // The start line, as a little chequer.
        let (sx, sy) = at(self.map.dots[0]);
        let q = (1.6 * s).max(2.0);
        for (i, j) in [(0.0f32, 0.0f32), (1.0, 1.0)] {
            hud.rect((sx - q + i * q).floor(), (sy - q + j * q).floor(), q, q, [0.05, 0.05, 0.05, 1.0]);
        }
        for (i, b) in self.bots.iter().enumerate().rev() {
            let p = at(self.map.unit(Vec2::new(b.car.pos().x, b.car.pos().z)));
            dot(hud, p, 5.5 * s, [0.0, 0.0, 0.0, 0.8]);
            dot(hud, p, 4.0 * s, kart_colour(i + 1));
        }
        let p = at(self.map.unit(self.my_pos));
        dot(hud, p, 7.5 * s, [1.0, 1.0, 1.0, 0.95]);
        dot(hud, p, 5.5 * s, kart_colour(0));
    }
}

pub(super) fn ordinal(n: usize) -> String {
    let tail = match n {
        1 => "ST",
        2 => "ND",
        3 => "RD",
        _ => "TH",
    };
    format!("{n}{tail}")
}

/// Seconds as minutes:seconds, the seconds to `places` decimal places:
/// "1:23.4" on the race's HUD, "1:02.35" on a course card.
pub(crate) fn clock(t: f32, places: usize) -> String {
    format!("{}:{:0w$.p$}", (t / 60.0).floor() as u32, t % 60.0, w = places + 3, p = places)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_reads_minutes_and_tenths() {
        assert_eq!(clock(83.45, 1), "1:23.4");
        assert_eq!(clock(62.35, 2), "1:02.35");
        assert_eq!(ordinal(2), "2ND");
    }
}
