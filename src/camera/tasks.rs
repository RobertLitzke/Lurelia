//! Photo tasks: a queue of five, each a subject and up to two more needs
//! (frame shape, time of day, angle, what the animal is doing, where it sits
//! in the frame, what is behind it). A finished task is replaced by the next
//! one from a seeded generator, so every player of a world gets the same
//! queue. There is no progression yet; seasons and weather will add needs.

use super::lens::Orient;
use super::scene::{Doing, Seen, Shot};
use crate::world::noise::{hash32, Rng};
use serde::{Deserialize, Serialize};

pub const QUEUE: usize = 5;
/// A subject must be at least this tall in the frame (fraction of its height).
pub const MIN_SIZE: f32 = 0.05;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum When {
    /// Before 9:00.
    Morning,
    /// 11:00 to 14:00.
    Midday,
    /// From 17:00 to dusk.
    GoldenHour,
}

impl When {
    pub fn holds(self, hour: f32) -> bool {
        match self {
            When::Morning => (crate::world::clock::dawn()..crate::render::daylight::path().solar(9.0)).contains(&hour),
            When::Midday => (11.0..14.0).contains(&hour),
            When::GoldenHour => (crate::render::daylight::path().solar(17.0)..=crate::world::clock::dusk()).contains(&hour),
        }
    }

    fn phrase(self) -> &'static str {
        match self {
            When::Morning => "IN THE MORNING (BEFORE 9:00)",
            When::Midday => "AT MIDDAY (11:00 TO 14:00)",
            When::GoldenHour => "IN THE GOLDEN HOUR (AFTER 17:00)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Angle {
    /// The subject at least 12 degrees above the camera.
    Up,
    /// At least 12 degrees below.
    Down,
    /// Within 6 degrees of level.
    Level,
}

impl Angle {
    fn holds(self, elev: f32) -> bool {
        match self {
            Angle::Up => elev >= 12.0,
            Angle::Down => elev <= -12.0,
            Angle::Level => elev.abs() <= 6.0,
        }
    }

    fn phrase(self) -> &'static str {
        match self {
            Angle::Up => "LOOKING UP AT IT",
            Angle::Down => "LOOKING DOWN ON IT",
            Angle::Level => "AT EYE LEVEL",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// On one of the four rule-of-thirds points.
    Thirds,
    Centred,
    /// At least half the frame's height.
    Fills,
    /// Small in a wide view: under 15% of the frame's height.
    Small,
}

impl Place {
    fn holds(self, s: &Seen) -> bool {
        match self {
            Place::Thirds => [1.0 / 3.0, 2.0 / 3.0].iter().any(|&a| (s.u - a).abs() < 0.09) && [1.0 / 3.0, 2.0 / 3.0].iter().any(|&b| (s.v - b).abs() < 0.09),
            Place::Centred => (s.u - 0.5).abs() < 0.1 && (s.v - 0.5).abs() < 0.1,
            Place::Fills => s.size >= 0.5,
            Place::Small => s.size < 0.15,
        }
    }

    fn phrase(self) -> &'static str {
        match self {
            Place::Thirds => "ON A THIRDS POINT",
            Place::Centred => "IN THE CENTRE",
            Place::Fills => "FILLING THE FRAME",
            Place::Small => "SMALL IN A WIDE VIEW",
        }
    }
}

/// How far off the subject is, in metres.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Range {
    /// Within 8 m: close enough that it has let you in.
    Close,
    /// 30 m or more: the animal small in the country it lives in.
    Far,
}

impl Range {
    fn holds(self, d: f32) -> bool {
        match self {
            Range::Close => d <= 8.0,
            Range::Far => d >= 30.0,
        }
    }

    fn phrase(self) -> &'static str {
        match self {
            Range::Close => "FROM CLOSE IN (UNDER 8 M)",
            Range::Far => "FROM WELL BACK (OVER 30 M)",
        }
    }
}

/// Which lens it was taken on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Glass {
    Wide,
    Long,
}

impl Glass {
    fn holds(self, focal_mm: f32) -> bool {
        match self {
            Glass::Wide => focal_mm <= 35.0,
            Glass::Long => focal_mm >= 100.0,
        }
    }

    fn phrase(self) -> &'static str {
        match self {
            Glass::Wide => "ON A WIDE LENS (35 MM OR LESS)",
            Glass::Long => "ON A LONG LENS (100 MM OR MORE)",
        }
    }
}

/// One requirement of a task.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(tag = "need", rename_all = "snake_case")]
pub enum Need {
    /// A sharp, unhidden creature or tree of this name in the frame.
    Subject {
        name: String,
        plant: bool,
    },
    /// At least this many of the subject in the frame.
    Count {
        n: u8,
    },
    Orientation {
        orient: Orient,
    },
    Time {
        when: When,
    },
    Angle {
        angle: Angle,
    },
    Doing {
        doing: Doing,
    },
    Place {
        place: Place,
    },
    /// Open sky behind the subject.
    Sky,
    /// The background well out of focus behind a sharp subject.
    SoftBackground,
    /// Water somewhere in the frame.
    Water,
    /// How far off the subject is.
    Range {
        range: Range,
    },
    /// The lens it was taken on.
    Glass {
        glass: Glass,
    },
    /// Stopped well down, so the whole scene is sharp.
    Deep,
    /// Something else of this name visible in the frame too.
    Companion {
        name: String,
    },
}

impl Need {
    /// A line for the logbook.
    pub fn line(&self) -> String {
        match self {
            Need::Subject { name, plant } => {
                format!("{}: {name}", if *plant { "PLANT" } else { "ANIMAL" })
            }
            Need::Count { n } => format!("AT LEAST {n} IN THE FRAME"),
            Need::Orientation { orient } => format!("{} FRAME", orient.name()),
            Need::Time { when } => when.phrase().to_string(),
            Need::Angle { angle } => angle.phrase().to_string(),
            Need::Doing { doing } => doing.phrase().to_string(),
            Need::Place { place } => place.phrase().to_string(),
            Need::Sky => "AGAINST THE SKY".to_string(),
            Need::SoftBackground => "BACKGROUND SOFT (WIDE APERTURE)".to_string(),
            Need::Water => "WATER IN THE FRAME".to_string(),
            Need::Range { range } => range.phrase().to_string(),
            Need::Glass { glass } => glass.phrase().to_string(),
            Need::Deep => "EVERYTHING SHARP (F/11 OR SMALLER)".to_string(),
            Need::Companion { name } => format!("WITH A {name} IN THE FRAME"),
        }
    }

    /// What is missing, said against what the photo actually shows.
    fn miss_for(&self, s: &Seen) -> String {
        match self {
            Need::Doing { doing } => match s.doing {
                Some(d) => format!("{} WAS {}, NOT {}", s.name, d.phrase(), doing.phrase()),
                None => self.line(),
            },
            Need::Range { range } => format!("{} WAS {:.0} M OFF: {}", s.name, s.dist, range.phrase()),
            _ => self.line(),
        }
    }

    /// Needs about the subject itself, judged per creature.
    fn about_subject(&self) -> bool {
        matches!(
            self,
            Need::Angle { .. } | Need::Doing { .. } | Need::Place { .. } | Need::Sky | Need::SoftBackground | Need::Range { .. }
        )
    }

    fn holds_for(&self, s: &Seen) -> bool {
        match self {
            Need::Angle { angle } => angle.holds(s.elev),
            Need::Doing { doing } => s.doing == Some(*doing),
            Need::Place { place } => place.holds(s),
            Need::Sky => s.sky_behind,
            Need::SoftBackground => s.bg_blur >= 5.0,
            Need::Range { range } => range.holds(s.dist),
            _ => true,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: u32,
    pub title: String,
    pub needs: Vec<Need>,
    pub reward: u32,
}

impl Task {
    fn new(id: u32, needs: Vec<Need>) -> Self {
        let reward = match needs.len() {
            0 | 1 => 10,
            2 => 25,
            3 => 45,
            _ => 70,
        };
        Task {
            id,
            title: title(&needs),
            needs,
            reward,
        }
    }

    pub fn subject(&self) -> Option<&str> {
        self.needs.iter().find_map(|n| match n {
            Need::Subject { name, .. } => Some(name.as_str()),
            _ => None,
        })
    }

    /// Whether `shot` does it; if not, what is missing (best candidate first).
    pub fn check(&self, shot: &Shot) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();
        for n in &self.needs {
            let ok = match n {
                Need::Orientation { orient } => shot.orient == *orient,
                Need::Time { when } => when.holds(shot.hour),
                Need::Water => shot.water,
                Need::Glass { glass } => glass.holds(shot.lens.focal_mm),
                Need::Deep => shot.lens.f_number() >= 11.0,
                Need::Companion { name } => shot.seen.iter().any(|s| s.is(name) && s.in_frame() && s.visible && s.size >= MIN_SIZE),
                _ => true,
            };
            if !ok {
                missing.push(n.line());
            }
        }
        if let Some(name) = self.subject() {
            let framed: Vec<&Seen> = shot.seen.iter().filter(|s| s.is(name) && s.in_frame()).collect();
            let good: Vec<&Seen> = framed.iter().copied().filter(|s| s.visible && s.sharp && s.size >= MIN_SIZE).collect();
            if good.is_empty() {
                missing.push(match framed.iter().max_by(|a, b| a.size.total_cmp(&b.size)) {
                    None => format!("NO {name} IN THE FRAME"),
                    Some(s) if !s.visible => format!("THE {name} IS HIDDEN"),
                    Some(s) if s.size < MIN_SIZE => format!("THE {name} IS TOO SMALL"),
                    Some(_) => format!("THE {name} IS OUT OF FOCUS"),
                });
                return Err(missing);
            }
            // Count: how many of them show at all, as long as one is sharp.
            for n in &self.needs {
                if let Need::Count { n: want } = n {
                    let shown = framed.iter().filter(|s| s.visible).count();
                    if shown < *want as usize {
                        missing.push(format!("ONLY {shown} OF {want}"));
                    }
                }
            }
            // The rest must all hold for the same creature: the best one.
            let about: Vec<&Need> = self.needs.iter().filter(|n| n.about_subject()).collect();
            let best = good.iter().max_by_key(|s| about.iter().filter(|n| n.holds_for(s)).count()).unwrap();
            for n in about {
                if !n.holds_for(best) {
                    missing.push(n.miss_for(best));
                }
            }
        }
        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }
}

/// A short title from the needs: "RED FOX EATING".
fn title(needs: &[Need]) -> String {
    let mut parts = Vec::new();
    let mut subject = None;
    for n in needs {
        match n {
            Need::Subject { name, .. } => subject = Some(name.clone()),
            Need::Count { n } => parts.push(format!("({n} OR MORE)")),
            Need::Doing { doing } => parts.push(doing.phrase().to_string()),
            Need::Sky => parts.push("AGAINST THE SKY".into()),
            Need::Angle { angle: Angle::Up } => parts.push("FROM BELOW".into()),
            Need::Angle { angle: Angle::Down } => parts.push("FROM ABOVE".into()),
            Need::Place { place: Place::Fills } => parts.push("CLOSE UP".into()),
            Need::Place { place: Place::Small } => parts.push("IN ITS LANDSCAPE".into()),
            Need::Time { when: When::GoldenHour } => parts.push("AT GOLDEN HOUR".into()),
            Need::Time { when: When::Morning } => parts.push("IN THE MORNING".into()),
            Need::Water if subject.is_some() => parts.push("BY THE WATER".into()),
            Need::Range { range: Range::Close } => parts.push("UP CLOSE".into()),
            Need::Range { range: Range::Far } => parts.push("FROM A DISTANCE".into()),
            Need::Glass { glass: Glass::Wide } => parts.push("ON A WIDE LENS".into()),
            Need::Glass { glass: Glass::Long } => parts.push("ON A LONG LENS".into()),
            Need::Companion { name } => parts.push(format!("WITH A {name}")),
            _ => {}
        }
    }
    let head = match subject {
        Some(s) => s,
        None if needs.iter().any(|n| matches!(n, Need::Water)) => "WATER SCENE".to_string(),
        None => "LANDSCAPE".to_string(),
    };
    let mut t = head;
    for p in parts.into_iter().take(2) {
        t.push(' ');
        t.push_str(&p);
    }
    t
}

/// What a task may ask about: the world's creatures and trees.
#[derive(Clone, Debug, Default)]
pub struct Pool {
    pub animals: Vec<Animal>,
    pub plants: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Animal {
    pub name: String,
    pub flies: bool,
    /// Grazes, drinks and rests (rigged walkers).
    pub walks: bool,
    /// Usually seen in groups.
    pub groups: bool,
}

/// The `n`th task of a world with `seed`.
pub fn make(seed: u32, n: u32, pool: &Pool) -> Task {
    let mut rng = Rng::new(hash32(seed ^ 0x7a5c_31d1 ^ n.wrapping_mul(0x9e37_79b9)) | 1);
    let pick = |rng: &mut Rng, k: usize| ((rng.unit() * k as f32) as usize).min(k.saturating_sub(1));
    // How many needs, subject included: a bare "DEER" is now the rare case
    // and most tasks ask for two or three things on top of the subject.
    // 1 (12%), 2 (33%), 3 (35%), 4 (20%).
    let r = rng.unit();
    let total = if r < 0.12 {
        1
    } else if r < 0.45 {
        2
    } else if r < 0.80 {
        3
    } else {
        4
    };
    let kind = rng.unit();
    let mut needs = Vec::new();
    let mut options: Vec<Need> = Vec::new();
    let times = [When::Morning, When::Midday, When::GoldenHour];
    let orients = [Orient::Landscape, Orient::Portrait, Orient::Square];
    if (kind < 0.8 || pool.plants.is_empty()) && !pool.animals.is_empty() {
        let a = &pool.animals[pick(&mut rng, pool.animals.len())];
        needs.push(Need::Subject { name: a.name.clone(), plant: false });
        if a.flies {
            options.extend([Need::Doing { doing: Doing::Flying }, Need::Doing { doing: Doing::Perched }, Need::Sky, Need::Angle { angle: Angle::Up }]);
        } else if a.walks {
            options.extend([
                Need::Doing { doing: Doing::Eating },
                Need::Doing { doing: Doing::Resting },
                Need::Doing { doing: Doing::Moving },
                Need::Doing { doing: Doing::Drinking },
                Need::Angle { angle: Angle::Level },
            ]);
        } else {
            options.extend([Need::Doing { doing: Doing::Moving }, Need::Angle { angle: Angle::Down }]);
        }
        if a.groups {
            options.push(Need::Count { n: 2 + (rng.unit() * 2.0) as u8 });
        }
        options.extend([
            Need::Place { place: Place::Thirds },
            Need::Place { place: Place::Centred },
            Need::Place { place: Place::Fills },
            Need::Place { place: Place::Small },
            Need::SoftBackground,
            Need::Deep,
            Need::Water,
            Need::Range { range: Range::Close },
            Need::Range { range: Range::Far },
            Need::Glass { glass: Glass::Wide },
            Need::Glass { glass: Glass::Long },
            Need::Orientation { orient: orients[pick(&mut rng, 3)] },
            Need::Time { when: times[pick(&mut rng, 3)] },
        ]);
        // A tree in the frame with it, where the world has trees.
        if !pool.plants.is_empty() {
            options.push(Need::Companion {
                name: pool.plants[pick(&mut rng, pool.plants.len())].clone(),
            });
        }
    } else if kind < 0.9 && !pool.plants.is_empty() {
        let p = &pool.plants[pick(&mut rng, pool.plants.len())];
        needs.push(Need::Subject { name: p.clone(), plant: true });
        options.extend([
            Need::Orientation { orient: Orient::Portrait },
            Need::Orientation { orient: Orient::Square },
            Need::Angle { angle: Angle::Up },
            Need::Sky,
            Need::Place { place: Place::Thirds },
            Need::Place { place: Place::Fills },
            Need::Glass { glass: Glass::Wide },
            Need::Glass { glass: Glass::Long },
            Need::Range { range: Range::Close },
            Need::Deep,
            Need::SoftBackground,
            Need::Time { when: times[pick(&mut rng, 3)] },
        ]);
    } else {
        needs.push(Need::Water);
        options.extend([
            Need::Orientation { orient: orients[pick(&mut rng, 3)] },
            Need::Time { when: times[pick(&mut rng, 3)] },
            Need::Glass { glass: Glass::Wide },
            Need::Deep,
        ]);
    }
    while needs.len() < total && !options.is_empty() {
        let o = options.swap_remove(pick(&mut rng, options.len()));
        if !needs.iter().any(|n| clashes(n, &o)) {
            needs.push(o);
        }
    }
    Task::new(n, needs)
}

/// Two needs that can't both hold, or ask the same question twice.
fn clashes(a: &Need, b: &Need) -> bool {
    use std::mem::discriminant;
    if discriminant(a) == discriminant(b) {
        return true;
    }
    let pair = |x: &Need, y: &Need| match (x, y) {
        (Need::Count { .. }, Need::Place { place: Place::Fills }) => true,
        (Need::Place { place: Place::Small }, Need::SoftBackground) => true,
        (Need::Doing { doing: Doing::Flying }, Need::SoftBackground) => true,
        (Need::Sky, Need::SoftBackground) => true,
        (Need::Doing { doing: Doing::Perched }, Need::Sky) => true,
        // A long way off and filling the frame, or close in and small in it.
        (Need::Range { range: Range::Far }, Need::Place { place: Place::Fills }) => true,
        (Need::Range { range: Range::Close }, Need::Place { place: Place::Small }) => true,
        (Need::Range { range: Range::Far }, Need::Companion { .. }) => true,
        // A wide lens can't throw a background out, and a stopped-down
        // aperture is the opposite of a soft one.
        (Need::Glass { glass: Glass::Wide }, Need::SoftBackground) => true,
        (Need::Deep, Need::SoftBackground) => true,
        (Need::Glass { glass: Glass::Long }, Need::Place { place: Place::Small }) => true,
        (Need::Place { place: Place::Fills }, Need::Companion { .. }) => true,
        // Drinking already puts water in the frame, and never against the sky.
        (Need::Doing { doing: Doing::Drinking }, Need::Water) => true,
        (Need::Doing { doing: Doing::Drinking }, Need::Sky) => true,
        (Need::Doing { doing: Doing::Drinking }, Need::Angle { angle: Angle::Up }) => true,
        _ => false,
    };
    pair(a, b) || pair(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool() -> Pool {
        Pool {
            animals: vec![
                Animal {
                    name: "DEER".into(),
                    flies: false,
                    walks: true,
                    groups: true,
                },
                Animal {
                    name: "GULL".into(),
                    flies: true,
                    walks: false,
                    groups: true,
                },
            ],
            plants: vec!["PINE".into()],
        }
    }

    fn deer(u: f32, v: f32) -> Seen {
        Seen {
            name: "DEER".into(),
            doing: Some(Doing::Eating),
            u,
            v,
            size: 0.3,
            dist: 12.0,
            visible: true,
            sharp: true,
            bg_blur: 8.0,
            ..Default::default()
        }
    }

    /// The same seed and index give the same task; tasks have 1 to 3 needs and
    /// never two of a kind.
    #[test]
    fn tasks_are_seeded_and_well_formed() {
        let p = pool();
        assert_eq!(make(7, 3, &p), make(7, 3, &p));
        let all: Vec<Task> = (0..200).map(|n| make(7, n, &p)).collect();
        for t in &all {
            assert!((1..=4).contains(&t.needs.len()), "{t:?}");
            for (i, a) in t.needs.iter().enumerate() {
                for b in &t.needs[i + 1..] {
                    assert!(!clashes(a, b), "{t:?}");
                }
            }
            assert!(!t.title.is_empty());
        }
        assert!(all.iter().any(|t| t.needs.len() == 4) && all.iter().any(|t| t.needs.len() == 1));
        // Most tasks ask for more than a bare subject.
        let bare = all.iter().filter(|t| t.needs.len() == 1).count();
        assert!(bare < all.len() / 5, "{bare} of {} tasks are a bare subject", all.len());
        assert!(all.iter().any(|t| t.needs.iter().any(|n| matches!(n, Need::Subject { plant: true, .. }))));
        let mut empty = Pool::default();
        assert_eq!(make(1, 1, &empty).needs[0], Need::Water);
        empty.plants.push("OAK".into());
        assert!(matches!(make(1, 1, &empty).needs[0], Need::Subject { plant: true, .. } | Need::Water));
    }

    /// A shot passes when every need holds for one sharp, visible subject, and
    /// says what is missing when it doesn't.
    #[test]
    fn checks_report_what_is_missing() {
        let t = Task::new(
            0,
            vec![
                Need::Subject { name: "DEER".into(), plant: false },
                Need::Doing { doing: Doing::Eating },
                Need::Place { place: Place::Thirds },
            ],
        );
        let mut shot = Shot {
            seen: vec![deer(0.34, 0.66)],
            ..Default::default()
        };
        assert_eq!(t.check(&shot), Ok(()));
        shot.seen[0].u = 0.5;
        assert_eq!(t.check(&shot), Err(vec!["ON A THIRDS POINT".to_string()]));
        shot.seen[0].sharp = false;
        assert_eq!(t.check(&shot), Err(vec!["THE DEER IS OUT OF FOCUS".to_string()]));
        shot.seen[0].u = 1.4;
        assert_eq!(t.check(&shot), Err(vec!["NO DEER IN THE FRAME".to_string()]));

        let herd = Task::new(
            1,
            vec![
                Need::Subject { name: "DEER".into(), plant: false },
                Need::Count { n: 3 },
                Need::Orientation { orient: Orient::Portrait },
            ],
        );
        let shot = Shot {
            orient: Orient::Portrait,
            seen: vec![deer(0.3, 0.5), deer(0.6, 0.5)],
            ..Default::default()
        };
        assert_eq!(herd.check(&shot), Err(vec!["ONLY 2 OF 3".to_string()]));
        let mut three = shot.clone();
        three.seen.push(Seen { sharp: false, ..deer(0.8, 0.5) });
        assert_eq!(herd.check(&three), Ok(()));

        // A silver fox counts as a fox.
        let fox = Task::new(2, vec![Need::Subject { name: "FOX".into(), plant: false }]);
        let shot = Shot {
            seen: vec![Seen {
                name: "SILVER FOX".into(),
                like: Some("FOX".into()),
                ..deer(0.5, 0.5)
            }],
            ..Default::default()
        };
        assert_eq!(fox.check(&shot), Ok(()));

        let water = Task::new(3, vec![Need::Water, Need::Time { when: When::GoldenHour }]);
        let shot = Shot {
            water: true,
            hour: 15.0,
            ..Default::default()
        };
        assert_eq!(water.check(&shot), Err(vec!["IN THE GOLDEN HOUR (AFTER 17:00)".to_string()]));
    }
}
