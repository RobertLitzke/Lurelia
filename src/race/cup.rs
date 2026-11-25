//! A cup: three races on three courses, points for each place, and the
//! table between races. Each course is its own world, so the cup goes from
//! one world to the next in the run's `launch::Chosen`: the race scores it
//! when its karts are home, and Enter on the results hands it back
//! (`Race::take_next`) for main.rs to load the next course with.
//!
//! The karts are the same from race to race: kart 0 is the player, rival
//! `i` is always the same colour in the same kart (game.rs `start_race`),
//! and a cup holds the number of rivals its first course is written for.

/// Points for each place, first to eighth.
const POINTS: [u32; 8] = [10, 8, 6, 5, 4, 3, 2, 1];
/// Races in a cup, or every course if there are fewer.
pub const RACES: usize = 3;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cup {
    /// The courses' worlds, in the order they are raced.
    pub courses: Vec<String>,
    /// Which of them is being raced now.
    pub at: usize,
    /// Points so far, by kart: 0 the player, then each rival.
    pub points: Vec<u32>,
    /// Each race run so far: the karts in the order they finished.
    pub results: Vec<Vec<usize>>,
}

impl Cup {
    /// A cup starting on the course in `first`, then the next ones in the
    /// race menu's order (`all`), wrapping round.
    pub fn new(first: &str, all: &[String]) -> Cup {
        let start = all.iter().position(|w| w == first).unwrap_or(0);
        let mut courses: Vec<String> = (0..all.len().min(RACES)).map(|i| all[(start + i) % all.len()].clone()).collect();
        if courses.is_empty() {
            courses.push(first.to_string());
        }
        Cup { courses, ..Cup::default() }
    }

    /// Race `at` scored: its karts in race order.
    pub fn scored(&self) -> bool {
        self.results.len() > self.at
    }

    /// Score the race being run, once: `order` is the karts in race order.
    pub fn score(&mut self, order: &[usize]) {
        if self.scored() {
            return;
        }
        let n = order.iter().max().map_or(0, |m| m + 1);
        if self.points.len() < n {
            self.points.resize(n, 0);
        }
        for (place, &k) in order.iter().enumerate() {
            self.points[k] += POINTS.get(place).copied().unwrap_or(0);
        }
        self.results.push(order.to_vec());
    }

    /// The last race of the cup.
    pub fn last(&self) -> bool {
        self.at + 1 >= self.courses.len()
    }

    /// What follows a scored race: the cup on its next course, or a new cup
    /// over the same courses once this one is done.
    pub fn next(&self) -> Cup {
        if self.last() {
            Cup { courses: self.courses.clone(), ..Cup::default() }
        } else {
            Cup { at: self.at + 1, ..self.clone() }
        }
    }

    /// The karts by points, most first; a tie goes to the better place in
    /// the latest race.
    pub fn table(&self) -> Vec<(usize, u32)> {
        let latest = |k: usize| self.results.last().and_then(|r| r.iter().position(|&j| j == k)).unwrap_or(usize::MAX);
        let mut t: Vec<(usize, u32)> = self.points.iter().copied().enumerate().collect();
        t.sort_by(|a, b| b.1.cmp(&a.1).then(latest(a.0).cmp(&latest(b.0))));
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worlds() -> Vec<String> {
        ["peregrine", "ribstrand", "vermilion"].map(String::from).to_vec()
    }

    /// Three courses from the one picked, wrapping; points add up and are
    /// scored once a race.
    #[test]
    fn a_cup_runs_three_courses_and_scores_each_once() {
        let mut c = Cup::new("ribstrand", &worlds());
        assert_eq!(c.courses, ["ribstrand", "vermilion", "peregrine"]);
        c.score(&[2, 0, 1]);
        c.score(&[0, 1, 2]);
        assert_eq!(c.points, [8, 6, 10]);
        let mut c = c.next();
        assert_eq!((c.at, c.scored()), (1, false));
        c.score(&[0, 1, 2]);
        assert_eq!(c.table()[0], (0, 18));
        let c = c.next();
        assert!(c.last());
        let again = c.next();
        assert_eq!((again.at, again.points.len()), (0, 0), "a finished cup starts over");
    }

    /// Level on points, the better place in the latest race is ahead.
    #[test]
    fn a_tie_goes_to_the_latest_race() {
        let mut c = Cup::new("peregrine", &worlds());
        c.score(&[0, 1]);
        let mut c = c.next();
        c.score(&[1, 0]);
        assert_eq!(c.points, [18, 18]);
        assert_eq!(c.table()[0].0, 1);
    }
}
