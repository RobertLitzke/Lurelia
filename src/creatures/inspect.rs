//! What the debug inspector (game/eco.rs) shows of one pinned animal and
//! that only the walker step knows: the pressure on it split by sense, the
//! `walkable` probes it cast, and a short log of its decisions with the
//! reason that won each. Only the pinned animal carries an `Inspect`, so the
//! rest pay nothing for it.

use super::startle::Senses;
use std::collections::VecDeque;

/// How many decisions the log keeps.
pub const LOG: usize = 20;

#[derive(Default)]
pub struct Inspect {
    /// The player's pressure this step, by sense, before a display's `holds`.
    pub player: Senses,
    /// A predator's pressure, and a bolting neighbour's.
    pub hunter: f32,
    pub herd: f32,
    /// Its flee distance this step, metres (startle.rs `Nerve::dist`, after
    /// dozing).
    pub flee_m: f32,
    /// The probes cast this step: heading (radians, walk.rs convention),
    /// reach in metres, and whether the way was clear.
    pub probes: Vec<(f32, f32, bool)>,
    /// What it is going after: prey, a carcass, a plant, water.
    pub target: Option<String>,
    /// (clock, line), newest last.
    pub log: VecDeque<(f32, String)>,
    /// The reason for a decision taken this step, logged when the step ends.
    pending: Option<String>,
    /// Its mood and activity when last looked at, to catch changes nobody
    /// gave a reason for.
    last: Option<(u8, Option<u8>)>,
}

impl Inspect {
    /// A decision and why it was taken.
    pub fn why(&mut self, line: String) {
        self.pending = Some(line);
    }

    /// Log what changed since the last step: the reason given for it, or
    /// just the change of mood. `state` is (mood, activity) as small numbers,
    /// `name` how to print them. The activity alone changing is not logged:
    /// it follows the mood a step later (anim/life.rs) and would say the
    /// same thing twice.
    pub fn settle(&mut self, now: f32, state: (u8, Option<u8>), name: impl FnOnce() -> String) {
        let changed = self.last.is_some_and(|l| l.0 != state.0);
        if let Some(line) = self.pending.take() {
            self.push(now, line);
        } else if changed {
            self.push(now, name());
        }
        self.last = Some(state);
    }

    fn push(&mut self, now: f32, line: String) {
        if self.log.len() == LOG {
            self.log.pop_front();
        }
        self.log.push_back((now, line));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_reasons_and_unexplained_changes_and_keeps_twenty() {
        let mut i = Inspect::default();
        i.settle(0.0, (0, None), || "idle".into());
        assert!(i.log.is_empty(), "the first look is not a change");
        i.why("flee: sight 2.0 from player".into());
        i.settle(1.0, (2, None), || "flee".into());
        i.settle(2.0, (0, Some(1)), || "idle: walk".into());
        i.settle(2.5, (0, Some(2)), || "idle: graze".into());
        let lines: Vec<&str> = i.log.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(lines, ["flee: sight 2.0 from player", "idle: walk"]);
        for k in 0..30 {
            i.why(format!("walk {k}"));
            i.settle(3.0 + k as f32, (1, None), String::new);
        }
        assert_eq!(i.log.len(), LOG);
        assert_eq!(i.log.back().unwrap().1, "walk 29");
    }
}
