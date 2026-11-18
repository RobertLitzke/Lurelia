//! What a bot is like: a few scalars every layer reads, and the table of
//! difficulties built out of them.
//!
//! None of these is health, damage or speed, which is what keeps a bot an
//! opponent rather than a cheat: a harder bot sees more, settles its aim
//! sooner, misses by less and is willing to try more, but it is shooting the
//! same gun out of the same body.
//!
//! The same struct is also a character. Two bots at one difficulty with
//! different `nerve` and `tactics` fight differently, which is most of what
//! makes a roster of named bots worth having.

/// The scalars. Each runs 0..1 unless it says otherwise.
#[derive(Clone, Copy, Debug)]
pub struct Personality {
    /// How much of the base aim error is taken off. 0 is a novice, 1 is a
    /// machine.
    pub aim: f32,
    /// Seconds a freshly seen target is aimed at badly for. This is reaction
    /// time done as settling rather than as a timer, so a bot rounding a
    /// corner sprays and then finds you, which is what a person does.
    pub settle: f32,
    /// The cosine of the half-angle it notices movement in: 0.7 is a narrow
    /// cone ahead, 0 is everything in front, below 0 is behind it as well.
    /// Anything under 0 is a cheat, and is named as one.
    pub peripheral: f32,
    /// How far it can see at all, metres.
    pub sight: f32,
    /// Willingness to strafe, dodge and jump while fighting.
    pub tactics: f32,
    /// How low health goes before it breaks off. 0 never retreats.
    pub nerve: f32,
    /// Share of the full cadence it fires at: a slower bot leaves gaps.
    pub cadence: f32,
    /// Whether it leads a moving target with a slow projectile at all.
    pub leads: bool,
}

/// The difficulties, easiest first. `peripheral` below zero on the last row
/// is the one cheat in the table: a field of view past 180 degrees.
pub const LEVELS: [(&str, Personality); 7] = [
    (
        "novice",
        Personality {
            aim: 0.0,
            settle: 1.6,
            peripheral: 0.72,
            sight: 40.0,
            tactics: 0.0,
            nerve: 0.0,
            cadence: 0.55,
            leads: false,
        },
    ),
    (
        "easy",
        Personality {
            aim: 0.2,
            settle: 1.2,
            peripheral: 0.6,
            sight: 55.0,
            tactics: 0.2,
            nerve: 0.2,
            cadence: 0.7,
            leads: false,
        },
    ),
    (
        "average",
        Personality {
            aim: 0.4,
            settle: 0.9,
            peripheral: 0.45,
            sight: 70.0,
            tactics: 0.4,
            nerve: 0.3,
            cadence: 0.82,
            leads: true,
        },
    ),
    (
        "skilled",
        Personality {
            aim: 0.58,
            settle: 0.7,
            peripheral: 0.3,
            sight: 85.0,
            tactics: 0.6,
            nerve: 0.35,
            cadence: 0.9,
            leads: true,
        },
    ),
    (
        "adept",
        Personality {
            aim: 0.72,
            settle: 0.5,
            peripheral: 0.18,
            sight: 100.0,
            tactics: 0.75,
            nerve: 0.4,
            cadence: 0.95,
            leads: true,
        },
    ),
    (
        "masterful",
        Personality {
            aim: 0.86,
            settle: 0.3,
            peripheral: 0.05,
            sight: 120.0,
            tactics: 0.9,
            nerve: 0.45,
            cadence: 1.0,
            leads: true,
        },
    ),
    (
        "godlike",
        Personality {
            aim: 0.96,
            settle: 0.15,
            peripheral: -0.2,
            sight: 150.0,
            tactics: 1.0,
            nerve: 0.5,
            cadence: 1.0,
            leads: true,
        },
    ),
];

/// The names a world may ask for, easiest first.
pub const NAMES: [&str; LEVELS.len()] = {
    let mut out = [""; LEVELS.len()];
    let mut i = 0;
    while i < LEVELS.len() {
        out[i] = LEVELS[i].0;
        i += 1;
    }
    out
};

/// The row called `name`.
pub fn level(name: &str) -> Option<Personality> {
    LEVELS.iter().find(|(n, _)| *n == name).map(|(_, p)| *p)
}

impl Default for Personality {
    /// The middle of the table.
    fn default() -> Self {
        LEVELS[2].1
    }
}

impl Personality {
    /// Nudge one row's scalars to make a character of it: `bold` toward
    /// pressing the attack, `steady` toward holding its aim.
    pub fn flavour(mut self, bold: f32, steady: f32) -> Self {
        self.tactics = (self.tactics + bold * 0.25).clamp(0.0, 1.0);
        self.nerve = (self.nerve - bold * 0.12).clamp(0.0, 0.8);
        self.aim = (self.aim + steady * 0.1).clamp(0.0, 1.0);
        self.settle = (self.settle * (1.0 - steady * 0.25)).max(0.05);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table gets harder down every axis, and only the last row cheats.
    #[test]
    fn the_table_only_gets_harder() {
        for w in LEVELS.windows(2) {
            let (an, a) = w[0];
            let (bn, b) = w[1];
            assert!(b.aim > a.aim, "{bn} aims no better than {an}");
            assert!(b.settle < a.settle, "{bn} settles no sooner than {an}");
            assert!(b.peripheral < a.peripheral, "{bn} sees no wider than {an}");
            assert!(b.sight > a.sight, "{bn} sees no further than {an}");
            assert!(b.tactics >= a.tactics && b.cadence >= a.cadence);
        }
        let cheats = LEVELS.iter().filter(|(_, p)| p.peripheral < 0.0).count();
        assert_eq!(cheats, 1, "more than the top row is seeing behind itself");
        assert!(level("easy").is_some() && level("inhuman").is_none());
    }
}
