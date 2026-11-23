//! The player's best times on each course, kept in `records.json` in the
//! save folder: the best lap, and the best
//! race for each number of laps, with the kart each was set in. The race's
//! course cards show them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One time and the kart it was set in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Time {
    pub secs: f32,
    pub kart: String,
}

/// The bests on one course.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Best {
    pub lap: Option<Time>,
    /// By the number of laps, as text so the file reads plainly.
    pub race: BTreeMap<String, Time>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Records {
    /// By world name.
    pub courses: BTreeMap<String, Best>,
}

/// What a finished race beat.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Beaten {
    pub race: bool,
    pub lap: bool,
}

impl Records {
    fn path(save_dir: &Path) -> PathBuf {
        save_dir.join("records.json")
    }

    /// The records in `save_dir`; none where the file is missing or unreadable.
    pub fn load(save_dir: &Path) -> Records {
        std::fs::read_to_string(Self::path(save_dir)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self, save_dir: &Path) {
        let path = Self::path(save_dir);
        let Ok(t) = serde_json::to_string_pretty(self) else { return };
        let _ = std::fs::create_dir_all(save_dir);
        let tmp = path.with_extension("tmp");
        if let Err(e) = std::fs::write(&tmp, t).and_then(|_| std::fs::rename(&tmp, &path)) {
            eprintln!("records: {e}");
        }
    }

    pub fn best(&self, world: &str) -> Option<&Best> {
        self.courses.get(world)
    }

    /// A race finished on `world`: `laps` laps in `race` seconds, the best of
    /// them `lap`, in `kart`. Keeps whichever it beat.
    pub fn note(&mut self, world: &str, laps: u32, race: f32, lap: Option<f32>, kart: &str) -> Beaten {
        let b = self.courses.entry(world.to_string()).or_default();
        let mut out = Beaten::default();
        let t = |secs: f32| Time { secs, kart: kart.to_string() };
        let slot = b.race.entry(laps.to_string());
        match slot {
            std::collections::btree_map::Entry::Occupied(mut o) => {
                if race < o.get().secs {
                    o.insert(t(race));
                    out.race = true;
                }
            }
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(t(race));
                out.race = true;
            }
        }
        if let Some(l) = lap {
            if b.lap.as_ref().is_none_or(|o| l < o.secs) {
                b.lap = Some(t(l));
                out.lap = true;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a quicker time replaces one, and the file reads back as written.
    #[test]
    fn a_record_is_kept_until_beaten() {
        let mut r = Records::default();
        assert_eq!(r.note("peregrine", 3, 160.0, Some(52.0), "kart"), Beaten { race: true, lap: true });
        assert_eq!(r.note("peregrine", 3, 165.0, Some(51.0), "kart_heavy"), Beaten { race: false, lap: true });
        assert_eq!(r.note("peregrine", 1, 60.0, Some(55.0), "kart"), Beaten { race: true, lap: false });
        let b = r.best("peregrine").unwrap();
        assert_eq!(b.race["3"].secs, 160.0);
        assert_eq!(b.lap.as_ref().unwrap().kart, "kart_heavy");
        let dir = std::env::temp_dir().join(format!("lurelia_records_{}", std::process::id()));
        r.save(&dir);
        assert_eq!(Records::load(&dir), r);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
