//! Each individual's sex, rolled once when it is first dressed
//! (spawn.rs `roll_morphs`) and held in a `Sex` component. It is
//! shown in the crosshair readout, displays can ask for one or the other
//! (creatures/display), and a creature's `forms` may draw them with different
//! models (Species::model_for).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sex {
    Female,
    Male,
}

impl Sex {
    /// Even odds from a hash of the individual.
    pub fn roll(seed: u32) -> Sex {
        if crate::world::noise::hash32(seed ^ 0x5e75_1c0d) & 1 == 0 {
            Sex::Female
        } else {
            Sex::Male
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Sex::Female => "female",
            Sex::Male => "male",
        }
    }

    pub fn parse(s: &str) -> Option<Sex> {
        [Sex::Female, Sex::Male].into_iter().find(|x| x.name() == s)
    }
}
