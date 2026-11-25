//! What the ground is like to drive on: one class per material, shared by the
//! vehicles (grip and drag), the particles (what a wheel throws up) and the
//! sound. A material says which it is with `"surface"` in its JSON; one that
//! doesn't is guessed from its name, as the footstep sounds always were.

use crate::world::material::{self, AIR};
use crate::world::matlib::{self, flag};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Surface {
    Hardpack,
    Rock,
    /// Sand below the tide line: firm and fast.
    SandWet,
    /// Dune sand: bogs a slow wheel, planes a fast one.
    SandSoft,
    Grass,
    Mud,
    Snow,
    /// Black ice, glass, a skin of brine on salt: hardly any grip, long
    /// slides. Only a material that says so (`"surface": "ice"`) is ice.
    Ice,
}

/// How a surface takes a tyre.
#[derive(Clone, Copy, Debug)]
pub struct Feel {
    /// Peak grip, as a multiple of the tyre's own.
    pub grip: f32,
    /// Slip angle at the peak, radians.
    pub peak: f32,
    /// Rolling drag, m/s² at a crawl...
    pub drag: f32,
    /// ...plus this much more that goes as the wheel planes (`v_plane`).
    pub soft: f32,
}

impl Surface {
    pub const ALL: [Surface; 8] = [Surface::Hardpack, Surface::Rock, Surface::SandWet, Surface::SandSoft, Surface::Grass, Surface::Mud, Surface::Snow, Surface::Ice];

    pub fn name(self) -> &'static str {
        match self {
            Surface::Hardpack => "hardpack",
            Surface::Rock => "rock",
            Surface::SandWet => "sand_wet",
            Surface::SandSoft => "sand_soft",
            Surface::Grass => "grass",
            Surface::Mud => "mud",
            Surface::Snow => "snow",
            Surface::Ice => "ice",
        }
    }

    pub fn parse(s: &str) -> Option<Surface> {
        Surface::ALL.into_iter().find(|c| c.name() == s)
    }

    pub fn feel(self) -> Feel {
        let deg = |d: f32| d.to_radians();
        match self {
            Surface::Hardpack => Feel {
                grip: 1.0,
                peak: deg(9.0),
                drag: 0.25,
                soft: 0.0,
            },
            Surface::Rock => Feel {
                grip: 1.1,
                peak: deg(9.0),
                drag: 0.2,
                soft: 0.0,
            },
            Surface::SandWet => Feel {
                grip: 1.0,
                peak: deg(11.0),
                drag: 0.3,
                soft: 0.0,
            },
            Surface::SandSoft => Feel {
                grip: 0.75,
                peak: deg(16.0),
                drag: 0.5,
                soft: 2.6,
            },
            Surface::Grass => Feel {
                grip: 0.85,
                peak: deg(12.0),
                drag: 0.6,
                soft: 0.3,
            },
            Surface::Mud => Feel {
                grip: 0.6,
                peak: deg(18.0),
                drag: 1.2,
                soft: 1.5,
            },
            Surface::Snow => Feel {
                grip: 0.45,
                peak: deg(20.0),
                drag: 0.6,
                soft: 0.6,
            },
            Surface::Ice => Feel {
                grip: 0.3,
                peak: deg(6.0),
                drag: 0.12,
                soft: 0.0,
            },
        }
    }

    /// Whether a wheel turning on it throws anything up.
    pub fn loose(self) -> bool {
        !matches!(self, Surface::Rock | Surface::Hardpack | Surface::Ice)
    }

    /// Guess from a material's name, lower case with underscores or spaces.
    pub fn guess(name: &str) -> Surface {
        let n = name.to_ascii_lowercase();
        let has = |k: &[&str]| k.iter().any(|s| n.contains(s));
        if has(&["snow", "ice", "frost", "firn"]) {
            Surface::Snow
        } else if has(&["mud", "silt", "bog", "peat", "marsh", "clay"]) {
            Surface::Mud
        } else if has(&["sandstone"]) {
            Surface::Rock
        } else if has(&["sand", "dune"]) {
            if has(&["wet", "tidal", "flat"]) {
                Surface::SandWet
            } else {
                Surface::SandSoft
            }
        } else if has(&["grass", "moss", "turf", "meadow", "heath", "leaf", "leaves", "needle", "fern"]) {
            Surface::Grass
        } else if has(&["dirt", "soil", "loam", "gravel", "path", "track", "earth", "road", "ash", "shingle", "duff", "litter"]) {
            Surface::Hardpack
        } else {
            Surface::Rock
        }
    }

    /// The class of material `m`: what its JSON says, else a guess from its
    /// name. AIR, which no wheel stands on, counts as hardpack.
    pub fn of(m: u8) -> Surface {
        if m == AIR {
            return Surface::Hardpack;
        }
        if let Some(s) = matlib::surface(m) {
            return s;
        }
        match m {
            material::SAND => return Surface::SandSoft,
            material::SNOW => return Surface::Snow,
            material::GRASS => return Surface::Grass,
            material::DIRT => return Surface::Hardpack,
            _ => {}
        }
        if matlib::has(m, flag::FOLIAGE) || matlib::has(m, flag::UNDERSTORY) {
            return Surface::Grass;
        }
        matlib::name(m).map_or(Surface::Rock, Surface::guess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_guess_the_class() {
        assert_eq!(Surface::guess("WET SAND"), Surface::SandWet);
        assert_eq!(Surface::guess("dune_sand"), Surface::SandSoft);
        assert_eq!(Surface::guess("DUNE GRASS"), Surface::SandSoft);
        assert_eq!(Surface::guess("basalt"), Surface::Rock);
        assert_eq!(Surface::guess("navajo_sandstone"), Surface::Rock);
        assert_eq!(Surface::guess("forest_soil"), Surface::Hardpack);
        for s in Surface::ALL {
            assert_eq!(Surface::parse(s.name()), Some(s));
        }
    }
}
