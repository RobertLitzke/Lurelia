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
//! A different kind of match — a time limit, teams, one life each — is
//! another answer to the same three questions and belongs here beside this
//! one, not in the rules.

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

/// The match as it stands.
pub struct Round {
    /// Kills that win it; 0 plays on forever.
    target: u32,
    /// The board as it was when somebody got there, best first. None while
    /// the match is still being fought.
    over: Option<Vec<(String, Score)>>,
    /// Seconds the scoreboard has been up.
    shown: f32,
}

impl Round {
    pub fn new(target: u32) -> Round {
        Round { target, over: None, shown: 0.0 }
    }

    /// Kills that win, or None in a match that plays on forever.
    pub fn target(&self) -> Option<u32> {
        (self.target > 0).then_some(self.target)
    }

    /// True once the match is over: the world holds still behind the
    /// scoreboard, so the bots stop fighting over a finished match.
    pub fn frozen(&self) -> bool {
        self.over.is_some()
    }

    /// Who won, once there is a winner.
    pub fn winner(&self) -> Option<&str> {
        self.over.as_ref().and_then(|b| b.first()).map(|(n, _)| n.as_str())
    }

    /// Somebody died. `scores` is the board after the kill was counted, in
    /// fighter order; the match ends here if that kill was the last one it
    /// needed.
    pub fn after_kill(&mut self, _who: usize, _by: usize, scores: &[(String, Score)]) {
        if self.over.is_some() || self.target == 0 {
            return;
        }
        if scores.iter().any(|(_, s)| s.kills >= self.target) {
            let mut board: Vec<(String, Score)> = scores.to_vec();
            // Most kills first, then fewest deaths: a tie on the target score
            // goes to whoever died less getting there.
            board.sort_by_key(|(_, s)| (std::cmp::Reverse(s.kills), s.deaths));
            self.over = Some(board);
            self.shown = 0.0;
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
        let name_w = board.iter().map(|(n, _)| Hud::text_width(n, big)).fold(0.0, f32::max);
        let num_w = Hud::text_width("DEATHS", s).max(Hud::text_width("00", big));
        let col = 6.0 * s;
        let table = name_w + (num_w + col) * 2.0;
        let k_right = name_w + col + num_w;
        let d_right = k_right + col + num_w;
        for (i, (name, sc)) in board.iter().enumerate().rev() {
            let row = c.row(table, 13.0 * big);
            let tint = if i == 0 {
                won
            } else if name == "YOU" {
                ink
            } else {
                dim
            };
            let base = row.y + 1.0 * s;
            hud.label(row.x, base, big, tint, name);
            // Right-aligned on their own columns, so the numbers line up
            // however long the names are.
            let kills = sc.kills.to_string();
            let deaths = sc.deaths.to_string();
            hud.label(row.x + k_right - Hud::text_width(&kills, big), base, big, tint, &kills);
            hud.label(row.x + d_right - Hud::text_width(&deaths, big), base, big, tint, &deaths);
        }
        // The column headings sit over the table, so above it in the stack.
        let caption = [1.0, 0.97, 0.94, 0.45];
        let head = c.row(table, 12.0 * s);
        for (text, right) in [("KILLS", k_right), ("DEATHS", d_right)] {
            hud.label(head.x + right - Hud::text_width(text, s), head.y, s, caption, text);
        }
        c.gap(0.5);
        // A hairline under the heading, the width of the table.
        let rule = c.row(table, 2.0 * s);
        hud.rect(rule.x, rule.y, rule.w, (0.6 * s).max(1.0), [1.0, 0.86, 0.6, 0.5]);
        c.gap(0.4);
        let heading = match self.winner() {
            Some("YOU") => "YOU WIN".to_string(),
            Some(n) => format!("{n} WINS"),
            None => "MATCH OVER".to_string(),
        };
        c.text(hud, 2.6, won, &heading);
        if let Some(t) = self.target() {
            c.gap(0.4);
            c.text(hud, 1.0, dim, &format!("FIRST TO {t}"));
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
}
