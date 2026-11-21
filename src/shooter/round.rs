//! The match: how many kills win it, when it is over, the final scoreboard,
//! and playing again.
//!
//! The rules in `mod.rs` run a fight that never ends — bodies come back and
//! the score climbs. This is the one thing on top of that: a target score, and
//! what happens when somebody reaches it. It knows nothing about fighters,
//! guns or bots; it is handed the scores and answers three questions. The
//! world sets the target in its `"shooter"` block (`target_score`, 0 to play
//! on forever) and the launch screen overrides it (src/launch.rs).
//!
//! A different kind of match is another answer to the same three questions
//! and belongs here beside this one, not in the rules. Two of them are here:
//! team deathmatch, where a side's kills between them count, and capture the
//! flag, where captures do, either one optionally against a clock. Neither
//! knows what a flag is; the rules hand over the totals.

use crate::brain::Score;
use crate::game::Actions;
use crate::render::hud::Hud;
use crate::render::ui::{Corner, Side};

/// The least time the scoreboard is up before it will take a key, so the
/// click or shot that ended the match cannot start the next one.
const SETTLE_S: f32 = 0.9;

/// What the rules do when a match restarts. Applied by `mod.rs`, because
/// reviving a body needs the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reset {
    /// Put every score back to nothing.
    pub scores: bool,
}

/// What wins a match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Somebody's own kills reach the target: the deathmatch.
    Kills,
    /// A side's kills between them reach it: team deathmatch.
    TeamKills,
    /// A side's captures reach it: capture the flag.
    Captures,
}

/// The two sides' names and colours, for the board and the HUD. Side 0 is
/// the cold cyan that `avatar::TEAM_COLOURS` keeps first on purpose, and
/// side 1 the magenta third in that list, which is as far from it as the
/// list goes without touching the health lamp's amber.
pub const TEAM_NAMES: [&str; 2] = ["CYAN", "MAGENTA"];
pub const TEAM_INK: [[f32; 4]; 2] = [[0.55, 0.86, 0.95, 1.0], [0.92, 0.45, 0.93, 1.0]];

/// The final board: the rows, each with its side, and the sides' totals when
/// it was a team match.
#[derive(Clone, Debug)]
struct Board {
    rows: Vec<(String, Score, Option<u8>)>,
    totals: Option<[u32; 2]>,
    /// The side that won, or None for a draw, in a team match.
    side: Option<u8>,
}

/// The match as it stands.
pub struct Round {
    /// Kills or captures that win it; 0 plays on forever.
    target: u32,
    goal: Goal,
    /// Seconds a match lasts, 0 for no limit, and seconds played of it.
    time_s: f32,
    clock: f32,
    /// The board as it was when the match ended. None while it is still
    /// being fought.
    over: Option<Board>,
    /// Seconds the scoreboard has been up.
    shown: f32,
}

/// Each side's kills, from the board and who is on which side.
pub fn team_kills(scores: &[(String, Score)], teams: &[Option<u8>]) -> [u32; 2] {
    let mut t = [0, 0];
    for (i, (_, s)) in scores.iter().enumerate() {
        if let Some(Some(k)) = teams.get(i) {
            t[(*k as usize).min(1)] += s.kills;
        }
    }
    t
}

impl Round {
    pub fn new(target: u32) -> Round {
        Round::playing(Goal::Kills, target, 0.0)
    }

    pub fn playing(goal: Goal, target: u32, time_s: f32) -> Round {
        Round {
            target,
            goal,
            time_s: time_s.max(0.0),
            clock: 0.0,
            over: None,
            shown: 0.0,
        }
    }

    #[cfg(test)]
    pub fn goal(&self) -> Goal {
        self.goal
    }

    /// Kills or captures that win, or None in a match that plays on forever.
    pub fn target(&self) -> Option<u32> {
        (self.target > 0).then_some(self.target)
    }

    /// Seconds left on the clock, in a match that has one.
    pub fn left(&self) -> Option<f32> {
        (self.time_s > 0.0).then(|| (self.time_s - self.clock).max(0.0))
    }

    /// True once the match is over: the world holds still behind the
    /// scoreboard, so the bots stop fighting over a finished match.
    pub fn frozen(&self) -> bool {
        self.over.is_some()
    }

    /// Who won, once there is a winner: the top row of the board.
    pub fn winner(&self) -> Option<&str> {
        self.over.as_ref().and_then(|b| b.rows.first()).map(|(n, _, _)| n.as_str())
    }

    /// The side that won a team match that is over; None while it is being
    /// fought, for a draw, and in a deathmatch.
    #[cfg(test)]
    pub fn winning_side(&self) -> Option<u8> {
        self.over.as_ref().and_then(|b| b.side)
    }

    /// Put the board up for a team match: the winning side's rows first,
    /// each side best first.
    fn end_teams(&mut self, totals: [u32; 2], scores: &[(String, Score)], teams: &[Option<u8>]) {
        let side = match totals[0].cmp(&totals[1]) {
            std::cmp::Ordering::Greater => Some(0),
            std::cmp::Ordering::Less => Some(1),
            std::cmp::Ordering::Equal => None,
        };
        let first = side.unwrap_or(0);
        let mut rows: Vec<(String, Score, Option<u8>)> = scores.iter().enumerate().map(|(i, (n, s))| (n.clone(), *s, teams.get(i).copied().flatten())).collect();
        rows.sort_by_key(|(_, s, t)| (*t != Some(first), std::cmp::Reverse(s.caps), std::cmp::Reverse(s.kills), s.deaths));
        self.over = Some(Board { rows, totals: Some(totals), side });
        self.shown = 0.0;
    }

    /// The deathmatch board: most kills first, then fewest deaths, so a tie
    /// on the target goes to whoever died less getting there.
    fn end_alone(&mut self, scores: &[(String, Score)]) {
        let mut rows: Vec<(String, Score, Option<u8>)> = scores.iter().map(|(n, s)| (n.clone(), *s, None)).collect();
        rows.sort_by_key(|(_, s, _)| (std::cmp::Reverse(s.kills), s.deaths));
        self.over = Some(Board { rows, totals: None, side: None });
        self.shown = 0.0;
    }

    /// Somebody died. `scores` is the board after the kill was counted, in
    /// fighter order; the match ends here if that kill was the last one it
    /// needed.
    pub fn after_kill(&mut self, who: usize, by: usize, scores: &[(String, Score)]) {
        self.after_kill_in(who, by, scores, &[]);
    }

    /// The same, with each fighter's side, for team deathmatch.
    pub fn after_kill_in(&mut self, _who: usize, _by: usize, scores: &[(String, Score)], teams: &[Option<u8>]) {
        if self.over.is_some() || self.target == 0 {
            return;
        }
        match self.goal {
            Goal::Kills => {
                if scores.iter().any(|(_, s)| s.kills >= self.target) {
                    self.end_alone(scores);
                }
            }
            Goal::TeamKills => {
                let t = team_kills(scores, teams);
                if t.iter().any(|&k| k >= self.target) {
                    self.end_teams(t, scores, teams);
                }
            }
            Goal::Captures => {}
        }
    }

    /// A side scored. `totals` is both sides' captures after it.
    pub fn after_capture(&mut self, totals: [u32; 2], scores: &[(String, Score)], teams: &[Option<u8>]) {
        if self.over.is_some() || self.goal != Goal::Captures || self.target == 0 {
            return;
        }
        if totals.iter().any(|&t| t >= self.target) {
            self.end_teams(totals, scores, teams);
        }
    }

    /// The clock, in a match that has one: when it runs out the side ahead
    /// wins and a level match is a draw. `totals` is whatever the goal
    /// counts, captures or a side's kills.
    pub fn tick(&mut self, dt: f32, totals: [u32; 2], scores: &[(String, Score)], teams: &[Option<u8>]) {
        if self.over.is_some() || self.time_s <= 0.0 {
            return;
        }
        self.clock += dt;
        if self.clock >= self.time_s {
            match self.goal {
                Goal::Kills => self.end_alone(scores),
                _ => self.end_teams(totals, scores, teams),
            }
        }
    }

    /// One frame. Returns what the rules should put back when the player
    /// asks for another match.
    pub fn step(&mut self, dt: f32, _scores: &[(String, Score)], actions: &mut Actions) -> Option<Reset> {
        let again = std::mem::take(&mut actions.accept);
        if self.over.is_none() {
            return None;
        }
        self.shown += dt;
        if again && self.shown >= SETTLE_S {
            self.over = None;
            self.shown = 0.0;
            self.clock = 0.0;
            return Some(Reset { scores: true });
        }
        None
    }

    /// The final scoreboard, drawn over the HUD once the match is over and
    /// nothing at all before that.
    pub fn draw(&self, hud: &mut Hud, w: f32, h: f32, s: f32, _scores: &[(String, Score)]) {
        let Some(board) = &self.over else { return };
        // Everything the match was is behind this; dim it.
        hud.rect(0.0, 0.0, w, h, [0.04, 0.03, 0.05, 0.7]);
        let ink = [1.0, 0.97, 0.94, 0.95];
        let dim = [1.0, 0.97, 0.94, 0.6];
        let won = [1.0, 0.86, 0.6, 1.0];
        // The columns this match counts: captures first in capture the flag.
        let mut cols: Vec<(&str, fn(&Score) -> u32)> = Vec::new();
        if self.goal == Goal::Captures {
            cols.push(("CAPS", |s: &Score| s.caps));
        }
        cols.push(("KILLS", |s: &Score| s.kills));
        cols.push(("DEATHS", |s: &Score| s.deaths));
        // The stack grows upward, so this is drawn bottom first: the footer,
        // then the places from last to first, then the heading on top. The
        // screen is passed short so the block sits above the middle.
        let mut c = Corner::new(Side::BottomMiddle, w, h * 0.78, s);
        c.text(hud, 1.0, dim, "ENTER TO PLAY AGAIN      ESC FOR THE MENU");
        c.gap(1.0);
        // One size for every row, the winner told apart by its colour, so
        // the name column is the same width for all of them. The number
        // columns are as wide as their own headings, which are wider than
        // any score.
        let big = s * 1.5;
        let name_w = board.rows.iter().map(|(n, _, _)| Hud::text_width(n, big)).fold(0.0, f32::max);
        let num_w = Hud::text_width("DEATHS", s).max(Hud::text_width("00", big));
        let col = 6.0 * s;
        let table = name_w + (num_w + col) * cols.len() as f32;
        let right = |k: usize| name_w + (col + num_w) * (k as f32 + 1.0);
        for (i, (name, sc, team)) in board.rows.iter().enumerate().rev() {
            let row = c.row(table, 13.0 * big);
            // In a team match a row is its side's colour, the player's own
            // row at full strength.
            let tint = match team {
                Some(t) => {
                    let k = TEAM_INK[(*t as usize).min(1)];
                    [k[0], k[1], k[2], if name == "YOU" { 1.0 } else { 0.75 }]
                }
                None if i == 0 => won,
                None if name == "YOU" => ink,
                None => dim,
            };
            let base = row.y + 1.0 * s;
            hud.label(row.x, base, big, tint, name);
            // Right-aligned on their own columns, so the numbers line up
            // however long the names are.
            for (k, (_, f)) in cols.iter().enumerate() {
                let v = f(sc).to_string();
                hud.label(row.x + right(k) - Hud::text_width(&v, big), base, big, tint, &v);
            }
        }
        // The column headings sit over the table, so above it in the stack.
        let caption = [1.0, 0.97, 0.94, 0.45];
        let head = c.row(table, 12.0 * s);
        for (k, (text, _)) in cols.iter().enumerate() {
            hud.label(head.x + right(k) - Hud::text_width(text, s), head.y, s, caption, text);
        }
        c.gap(0.5);
        // A hairline under the heading, the width of the table.
        let rule = c.row(table, 2.0 * s);
        hud.rect(rule.x, rule.y, rule.w, (0.6 * s).max(1.0), [1.0, 0.86, 0.6, 0.5]);
        c.gap(0.4);
        let (heading, colour) = match (board.totals, board.side) {
            (Some(t), Some(side)) => {
                let k = side as usize;
                (format!("{} WINS {} - {}", TEAM_NAMES[k], t[k], t[1 - k]), TEAM_INK[k])
            }
            (Some(t), None) => (format!("A DRAW, {} - {}", t[0], t[1]), won),
            _ => match self.winner() {
                Some("YOU") => ("YOU WIN".to_string(), won),
                Some(n) => (format!("{n} WINS"), won),
                None => ("MATCH OVER".to_string(), won),
            },
        };
        c.text(hud, 2.6, colour, &heading);
        if let Some(t) = self.target() {
            c.gap(0.4);
            let what = match self.goal {
                Goal::Captures => format!("FIRST TO {t} CAPTURES"),
                _ => format!("FIRST TO {t}"),
            };
            c.text(hud, 1.0, dim, &what);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(kills: &[u32]) -> Vec<(String, Score)> {
        kills
            .iter()
            .enumerate()
            .map(|(i, k)| {
                (
                    if i == 0 { "YOU".to_string() } else { format!("BOT{i}") },
                    Score {
                        kills: *k,
                        deaths: 1,
                        ..Default::default()
                    },
                )
            })
            .collect()
    }

    #[test]
    fn the_match_ends_when_somebody_reaches_the_target() {
        let mut r = Round::new(3);
        r.after_kill(1, 0, &board(&[2, 0]));
        assert!(!r.frozen(), "two of three is not a win");
        r.after_kill(1, 0, &board(&[3, 0]));
        assert!(r.frozen() && r.winner() == Some("YOU"));
    }

    #[test]
    fn a_target_of_zero_plays_on_forever() {
        let mut r = Round::new(0);
        r.after_kill(1, 0, &board(&[99, 0]));
        assert!(!r.frozen() && r.target().is_none());
    }

    #[test]
    fn the_winner_is_whoever_has_the_most_kills() {
        let mut r = Round::new(5);
        r.after_kill(0, 2, &board(&[1, 3, 5]));
        assert_eq!(r.winner(), Some("BOT2"));
    }

    #[test]
    fn play_again_waits_out_the_shot_that_ended_it() {
        let mut r = Round::new(1);
        let b = board(&[1, 0]);
        r.after_kill(1, 0, &b);
        let mut a = Actions { accept: true, ..Default::default() };
        assert_eq!(r.step(0.1, &b, &mut a), None, "the key that ended the match restarted it");
        assert!(r.frozen());
        a.accept = true;
        assert_eq!(r.step(SETTLE_S, &b, &mut a), Some(Reset { scores: true }));
        assert!(!r.frozen(), "the match did not start again");
    }

    /// The scoreboard is drawn only once there is one, and it stays inside
    /// the screen it is given.
    #[test]
    fn the_scoreboard_comes_up_with_the_result_and_not_before() {
        let (w, h) = (1600.0, 900.0);
        let mut hud = Hud::default();
        let mut r = Round::new(2);
        let running = board(&[1, 0, 0]);
        r.draw(&mut hud, w, h, 2.0, &running);
        assert!(hud.items.is_empty(), "a match still being fought drew a scoreboard");
        let done = board(&[2, 0, 1]);
        r.after_kill(1, 0, &done);
        r.draw(&mut hud, w, h, 2.0, &done);
        assert!(hud.items.len() > 10, "the scoreboard drew almost nothing: {} items", hud.items.len());
        for it in &hud.items {
            let [x, y, iw, ih] = it.rect;
            assert!(x >= -1.0 && y >= -1.0 && x + iw <= w + 1.0 && y + ih <= h + 1.0, "the scoreboard ran off the screen: {:?}", it.rect);
        }
    }

    #[test]
    fn nothing_is_taken_from_the_actions_of_a_running_match() {
        let mut r = Round::new(5);
        let b = board(&[1, 0]);
        let mut a = Actions::default();
        a.accept = true;
        assert_eq!(r.step(0.1, &b, &mut a), None);
        assert!(!a.accept, "the match left Enter lying in the actions");
    }

    fn teams() -> Vec<Option<u8>> {
        vec![Some(0), Some(1), Some(0)]
    }

    /// In team deathmatch it is a side's kills between them that count, so
    /// nobody has to reach the target alone.
    #[test]
    fn a_side_wins_on_its_kills_between_them() {
        let mut r = Round::playing(Goal::TeamKills, 5, 0.0);
        r.after_kill_in(1, 0, &board(&[2, 4, 2]), &teams());
        assert!(!r.frozen(), "four kills of five ended it");
        r.after_kill_in(1, 0, &board(&[3, 4, 2]), &teams());
        assert!(r.frozen(), "the cyan side has five between them and played on");
        assert_eq!(r.winning_side(), Some(0));
    }

    #[test]
    fn captures_end_a_match_and_the_clock_ends_a_level_one_as_a_draw() {
        let mut r = Round::playing(Goal::Captures, 3, 60.0);
        r.after_capture([1, 2], &board(&[0, 0, 0]), &teams());
        assert!(!r.frozen());
        r.after_capture([1, 3], &board(&[0, 0, 0]), &teams());
        assert!(r.frozen() && r.winning_side() == Some(1));
        let mut r = Round::playing(Goal::Captures, 3, 1.0);
        r.tick(0.6, [1, 1], &board(&[0, 0, 0]), &teams());
        assert!(!r.frozen());
        r.tick(0.6, [1, 1], &board(&[0, 0, 0]), &teams());
        assert!(r.frozen() && r.winning_side().is_none(), "a level match on time was not a draw");
    }

    /// A team board is drawn in the sides' colours and stays on screen.
    #[test]
    fn a_team_board_stays_on_the_screen() {
        let (w, h) = (1600.0, 900.0);
        let mut hud = Hud::default();
        let mut r = Round::playing(Goal::Captures, 1, 0.0);
        r.after_capture([1, 0], &board(&[2, 1, 0]), &teams());
        r.draw(&mut hud, w, h, 2.0, &board(&[2, 1, 0]));
        assert!(hud.items.len() > 10);
        for it in &hud.items {
            let [x, y, iw, ih] = it.rect;
            assert!(x >= -1.0 && y >= -1.0 && x + iw <= w + 1.0 && y + ih <= h + 1.0, "the board ran off the screen: {:?}", it.rect);
        }
    }
}
