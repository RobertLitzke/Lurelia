//! Terrain classes: what kind of ground a point is, for rules that treat a
//! marsh differently from a street (`crate::tune`'s `terrain_pace`, and the
//! zombie round's director).
//!
//! Read off the survey's 4 m channels and the material underfoot rather than
//! off biome names, so a new biome falls into a class without anybody
//! listing it: shallow water is a marsh wherever it is, sand is a beach.

use super::{water, Chan, Survey};
use crate::world::store::Store;
use glam::Vec3;

/// The kinds of ground, first match wins in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Terrain {
    /// Ankle-deep or wading water, fresh or salt: a marsh, a ford, the surf.
    Marsh,
    /// Built up round about, or under a roof.
    Town,
    /// Sand underfoot.
    Beach,
    /// Canopy and undergrowth, the horizon closed in.
    Forest,
    /// Bare rock, scree, gravel.
    Rock,
    /// Wide open with nothing to hide in: salt flats, desert, steppe.
    Flats,
    /// Open grass.
    Meadow,
    /// None of those.
    Ground,
}

impl Terrain {
    pub const ALL: [Terrain; 8] = [Terrain::Marsh, Terrain::Town, Terrain::Beach, Terrain::Forest, Terrain::Rock, Terrain::Flats, Terrain::Meadow, Terrain::Ground];

    /// As a game file names it.
    pub fn name(self) -> &'static str {
        match self {
            Terrain::Marsh => "marsh",
            Terrain::Town => "town",
            Terrain::Beach => "beach",
            Terrain::Forest => "forest",
            Terrain::Rock => "rock",
            Terrain::Flats => "flats",
            Terrain::Meadow => "meadow",
            Terrain::Ground => "ground",
        }
    }
}

/// Words in a material's name that make it rock or sand underfoot.
const ROCKY: [&str; 10] = ["stone", "rock", "scree", "gravel", "talus", "basalt", "granite", "slate", "cinder", "flint"];
const SANDY: [&str; 2] = ["sand", "dune"];

/// The class of the ground a body stands on at `p` (its feet).
pub fn at(s: &Survey, store: &Store, p: Vec3) -> Terrain {
    let (x, z) = (p.x, p.z);
    if x < 0.0 || z < 0.0 || x >= s.size || z >= s.size {
        return Terrain::Ground;
    }
    let w = s.byte(Chan::Water, x, z);
    if matches!(w, water::FRESH_SHALLOW | water::FRESH_WADE | water::SALT_SHALLOW) {
        return Terrain::Marsh;
    }
    // Under a roof, or among buildings, is town whatever the floor is made
    // of: a stone floor indoors is not a scree slope.
    if s.value(Chan::Built, x, z) >= 0.2 || s.value(Chan::Roof, x, z) >= 0.3 {
        return Terrain::Town;
    }
    let under = crate::creatures::terrain::voxel_ground(store, x, z, p.y + 0.5).and_then(|(_, m)| crate::world::matlib::key(m));
    if under.is_some_and(|k| SANDY.iter().any(|w| k.contains(w))) {
        return Terrain::Beach;
    }
    let (cover, open) = (s.value(Chan::Cover, x, z), s.value(Chan::Open, x, z));
    if cover >= 0.45 && open < 0.5 {
        return Terrain::Forest;
    }
    if under.is_some_and(|k| ROCKY.iter().any(|w| k.contains(w))) {
        return Terrain::Rock;
    }
    let graze = s.value(Chan::Graze, x, z);
    if open >= 0.85 && cover < 0.08 && graze < 0.25 {
        return Terrain::Flats;
    }
    if graze >= 0.35 {
        return Terrain::Meadow;
    }
    Terrain::Ground
}
