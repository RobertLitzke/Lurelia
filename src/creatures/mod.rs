//! Living animals: who they are (`Species`, from the library), the components
//! every one carries (`Creature`, `Look`), and the movers that run them.
//! `Entities` (entities.rs) owns the hecs world and calls into these.
//!
//! - locomotion.rs: the movement verbs and which mover runs each.
//! - terrain.rs: ground, water and sea-bed queries the movers share.
//! - spawn.rs: keeping each biome's creatures around the player.
//! - walk.rs, herd.rs: walkers and their family groups.
//! - circle.rs, flock.rs, flit.rs: birds (and moths).
//! - swim.rs: fish and whales.
//! - snake.rs: snakes on the ground, in trees and in the water.
//! - fowl.rs, woodpecker.rs, insects.rs: waterfowl, woodpeckers, bees and
//!   dragonflies.
//! - runner.rs: running birds (shorebirds at the surf, plovers, roadrunners).
//! - hummer.rs: hummingbirds at flowers.
//! - birds.rs: wing animation for rigged birds.
//! - census.rs: counts and the creature under the crosshair.
//! - affordance.rs, plants.rs: what plants offer animals, and finding them.
//! - age.rs: the four ages, and how long bones lie.
//! - diel.rs: the hours each species keeps, and how drowsy it is outside them.
//! - carry.rs: prey a predator has caught, carried or dragged in its mouth.

pub mod affordance;
pub mod age;
pub mod birds;
pub mod carry;
pub mod census;
pub mod circle;
pub mod diel;
pub mod flit;
pub mod flock;
pub mod flung;
pub mod fowl;
pub mod herd;
pub mod hummer;
pub mod insects;
pub mod locomotion;
pub mod octopus;
pub mod plants;
pub mod runner;
pub mod sign;
pub mod snake;
pub mod spawn;
pub mod startle;
pub mod swim;
pub mod terrain;
pub mod walk;
pub mod woodpecker;

use crate::library::Behaviour;
use glam::Vec3;

/// A library creature, ready to spawn: its model and behaviour, with the
/// creatures it runs from and hunts resolved to species indices.
#[derive(Clone)]
pub struct Species {
    pub name: String,
    pub model: usize,
    pub beh: Behaviour,
    pub flees: Vec<usize>,
    pub hunts: Vec<usize>,
    pub flees_player: bool,
    /// The creature this one is a subspecies or morph of.
    pub like: Option<String>,
    /// The species whose biome quota its individuals fill: itself, except for
    /// a water form, which fills its land form's (a biome lists the animal
    /// once, whether it is on the bank or in the water).
    pub quota: usize,
    /// For a species that crosses between land and water, the other form of
    /// it: the water form of a land species and the land form of a water one.
    /// Built from the entry's `in_water` block, not from the library.
    pub water: Option<usize>,
    /// True on the generated water form, so the census and the logbook can
    /// name it as its land form rather than as an animal of its own.
    pub is_water_form: bool,
    /// How individuals differ (src/variety.rs).
    pub vary: crate::variety::Vary,
    /// The named coats it comes in, with the share of individuals wearing
    /// each (variety::Wear). Empty for a species that comes in one colouring.
    pub coats: Vec<crate::variety::Wear>,
    /// Rare morphs that stand in for this species at spawn: (species, chance).
    pub morphs: Vec<(usize, f32)>,
    /// Which ages its individuals come in (age.rs).
    pub ages: age::Stages,
    /// The tint that greys this species' coat, for an old individual
    /// (variety::aged_tint).
    pub aged_tint: [f32; 3],
}

impl Species {
    /// How it moves (locomotion.rs).
    pub fn verb(&self) -> &'static locomotion::Verb {
        locomotion::verb(&self.beh.movement)
    }
}

/// What to call species `s` to the player: a water form is the same animal as
/// its land form, so an alligator among the knees is an alligator and not an
/// `alligator@water`.
pub fn display_name(species: &[Species], s: usize) -> &str {
    let sp = &species[s];
    match sp.water {
        Some(land) if sp.is_water_form => &species[land].name,
        _ => &sp.name,
    }
}

/// Widen each species' `hunts` and `flees` to the relatives of what they
/// name: a creature `like` another (a subspecies or morph, at any depth) is
/// hunted and fled like its parent. A coyote that hunts rabbits hunts
/// jackrabbits; a rabbit that flees foxes flees silver foxes.
pub fn link_families(species: &mut [Species]) {
    let parent: Vec<Option<usize>> = species.iter().map(|s| s.like.as_ref().and_then(|p| species.iter().position(|x| &x.name == p))).collect();
    // Whether `i` is `of` or descends from it (the chain is cut at 8 steps).
    let descends = |mut i: usize, of: usize| {
        for _ in 0..8 {
            if i == of {
                return true;
            }
            match parent[i] {
                Some(p) => i = p,
                None => return false,
            }
        }
        false
    };
    let widen = |names: &[usize]| -> Vec<usize> { (0..parent.len()).filter(|&i| names.iter().any(|&n| descends(i, n))).collect() };
    let wide: Vec<(Vec<usize>, Vec<usize>)> = species.iter().map(|s| (widen(&s.hunts), widen(&s.flees))).collect();
    for (s, (hunts, flees)) in species.iter_mut().zip(wide) {
        s.hunts = hunts;
        s.flees = flees;
    }
}

/// Which coat an individual wears, and where in that coat's range it falls:
/// one draw for a whole group, so a herd is of a piece. Put on where the
/// group is placed (spawn::dress), because that is where the biome is known.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Coat {
    /// Index into the species' `coats`.
    pub which: usize,
    /// How far along the coat's range, 0..1.
    pub at: f32,
}

/// On every living library animal, whatever moves it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Creature {
    /// What it is (an index into `Entities::species`).
    pub species: usize,
    /// The species whose biome quota it fills: itself, or the parent it
    /// stands in for when it is a rare morph.
    pub spawned_for: usize,
}

impl Creature {
    pub fn of(species: usize) -> Self {
        Creature { species, spawned_for: species }
    }
}

/// Where an entity is drawn and with what model, now and one step ago.
pub struct Look {
    pub model: usize,
    pub pos: Vec3,
    pub prev_pos: Vec3,
    pub yaw: f32,
    pub prev_yaw: f32,
    pub bob: f32,
    pub flash: f32,
    /// Added to the drawn height so feet meet sloped ground (see
    /// physics::drawn_ground_lift); held while airborne.
    pub lift: f32,
    pub prev_lift: f32,
}

impl Look {
    pub fn new(model: usize, pos: Vec3, yaw: f32) -> Self {
        Look {
            model,
            pos,
            prev_pos: pos,
            yaw,
            prev_yaw: yaw,
            bob: 0.0,
            flash: 0.0,
            lift: 0.0,
            prev_lift: 0.0,
        }
    }

    /// Start a step: the current position and heading become the previous.
    pub fn begin_step(&mut self) {
        self.prev_pos = self.pos;
        self.prev_yaw = self.yaw;
    }
}

/// How walkers got on, for the headless stats: steps spent moving, steps
/// pushing against something, jumps, and steps standing on foliage.
#[derive(Clone, Copy, Default, Debug, serde::Serialize)]
pub struct Pathing {
    pub moving: u32,
    pub blocked: u32,
    pub jumps: u32,
    pub on_foliage: u32,
    pub turned: u32,
    pub stuck: u32,
    /// Bird-steps sitting on a river or the sea (should be none).
    pub birds_on_water: u32,
    /// Bird-steps sitting anywhere.
    pub birds_sitting: u32,
    /// Walker steps spent swimming a river.
    pub swimming_steps: u32,
    /// Wader-steps, and those within 4 m of a river or the shore.
    pub wader_steps: u32,
    pub wader_by_water: u32,
    /// Gull-steps, and those floating on the sea.
    pub gull_steps: u32,
    pub gull_floating: u32,
    /// Walks a plant-eater set off on toward browse, and the times one
    /// reached it and ate (creatures/walk.rs `BROWSE_M`).
    pub browse_walks: u32,
    pub browse_meals: u32,
    /// Flights taken by walkers that fly (herons), and steps spent in the air.
    pub flights: u32,
    pub flying_steps: u32,
    /// Steps a plodder spent drawn into its shell.
    pub hiding_steps: u32,
    /// Steps a walker spent drinking at the water's edge.
    pub drinking_steps: u32,
    /// Seals that slipped into the sea.
    pub dives: u32,
    /// Walker-steps that cost a cover raycast (startle.rs): the price of
    /// asking whether something stands between it and the player.
    pub cover_rays: u32,
    /// Walker-steps spent bolting from the player.
    pub startled: u32,
    /// Walker-steps where a neighbour bolting pressed hardest (startle.rs
    /// `Threat::alarm_of`): the alarm spreading through a herd.
    pub alarmed_by_herd: u32,
    /// Snake-steps (creatures/snake.rs), and those up a tree.
    pub snake_steps: u32,
    pub snakes_in_trees: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sp(name: &str, like: Option<&str>, hunts: Vec<usize>, flees: Vec<usize>) -> Species {
        let beh: Behaviour = serde_json::from_value(serde_json::json!({ "movement": "graze" })).unwrap();
        Species {
            name: name.into(),
            model: 0,
            beh,
            flees,
            hunts,
            flees_player: false,
            like: like.map(String::from),
            quota: 0,
            water: None,
            is_water_form: false,
            vary: Default::default(),
            coats: vec![],
            morphs: vec![],
            ages: Default::default(),
            aged_tint: [1.0; 3],
        }
    }

    #[test]
    fn hunters_and_the_hunted_take_in_relatives() {
        // 0 rabbit, 1 jackrabbit (like rabbit), 2 snowshoe (like jackrabbit),
        // 3 fox, 4 silver fox (like fox), 5 coyote hunting rabbits.
        let mut s = vec![
            sp("rabbit", None, vec![], vec![3]),
            sp("jackrabbit", Some("rabbit"), vec![], vec![5]),
            sp("snowshoe", Some("jackrabbit"), vec![], vec![]),
            sp("fox", None, vec![0], vec![]),
            sp("silver_fox", Some("fox"), vec![], vec![]),
            sp("coyote", None, vec![0], vec![]),
        ];
        link_families(&mut s);
        assert_eq!(s[5].hunts, vec![0, 1, 2]);
        assert_eq!(s[0].flees, vec![3, 4]);
        assert_eq!(s[1].flees, vec![5]);
        assert!(s[2].hunts.is_empty());
    }
}
