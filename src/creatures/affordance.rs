//! What a plant offers animals, named in a plant's `affords` list. This is
//! the one list of names; the library validates against `AFFORDANCES`, and
//! creature code asks for plants by `Affordance` through plants.rs.
//! Bees (insects.rs) look for `Nectar`; woodpeckers find trunks through
//! plants.rs without an affordance.

#![allow(dead_code)]

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Affordance {
    /// Leaves and twigs a grazer can eat.
    Browse,
    /// A branch or top a bird can sit on.
    Perch,
    /// Somewhere to raise young.
    Nest,
    /// Thick enough to hide in.
    Cover,
    /// Flowers that bees visit.
    Nectar,
}

pub const ALL: [Affordance; 5] = [Affordance::Browse, Affordance::Perch, Affordance::Nest, Affordance::Cover, Affordance::Nectar];

/// The names, in `ALL` order, for validation.
pub const AFFORDANCES: [&str; 5] = ["browse", "perch", "nest", "cover", "nectar"];

impl Affordance {
    pub fn name(self) -> &'static str {
        AFFORDANCES[self as usize]
    }
    pub fn parse(s: &str) -> Option<Affordance> {
        ALL.iter().copied().find(|a| a.name() == s)
    }
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// A set of affordances, one bit each.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Affords(u8);

impl Affords {
    /// From a plant's `affords` names; unknown names are skipped (validation
    /// reports them).
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a String>) -> Self {
        Affords(names.into_iter().filter_map(|n| Affordance::parse(n)).fold(0, |m, a| m | a.bit()))
    }
    pub fn has(self, a: Affordance) -> bool {
        self.0 & a.bit() != 0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_sets_hold_what_was_named() {
        for a in ALL {
            assert_eq!(Affordance::parse(a.name()), Some(a));
        }
        let s = Affords::from_names(&["perch".to_string(), "cover".to_string(), "fly".to_string()]);
        assert!(s.has(Affordance::Perch) && s.has(Affordance::Cover));
        assert!(!s.has(Affordance::Browse) && !s.has(Affordance::Nest));
        assert!(Affords::default().is_empty());
    }
}
