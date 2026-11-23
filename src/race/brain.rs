//! A rival's mind: how it drives on top of the style fitted to its kart, and
//! how it uses what it carries. Every kart in a race has one (the player's
//! too, when a bot drives it in a headless run). The game's rivals all use
//! `Brain::default()`; `Brain::original()` is the plain brain the tournament
//! (`tools/kart_brains/`) bred the default from, kept to race against.
//!
//! A brain is data, so variants can be raced against each other without a
//! build: `KART_BRAINS=file.json` gives each kart its brain by name (see
//! `Lineup`).

use serde::Deserialize;

/// How a kart's driver and trigger finger behave.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Brain {
    // Driving: shares of the style fitted to the kart (`Style::for_kart`).
    /// Sideways grip trusted in a bend.
    pub corner: f32,
    /// Braking trusted.
    pub brake: f32,
    /// Look-ahead seconds.
    pub look: f32,
    /// How far ahead a kart in its lane makes it move over, and how far to
    /// the side it passes.
    pub pass_look: f32,
    pub pass_side: f32,
    /// Over to a boost pad coming up when nobody is to be passed.
    pub pads: bool,
    /// With nothing carried, over to the item boxes coming up (within this
    /// many metres; 0 never).
    pub boxes: f32,
    /// Drift through bends at least this tight (1/m; 0 never), letting go
    /// for the boost as the bend opens.
    pub drift: f32,
    /// Tuck in behind a kart close ahead on a straight to ride its
    /// slipstream, rather than pull out to pass at once.
    pub draft: bool,

    // Items.
    /// The gun and rocket fired back at a kart close behind, and a disc
    /// thrown back at one, when there is nobody to shoot ahead.
    pub rear_fire: bool,
    /// Goo and blocks thrown ahead onto the line of a kart close in front,
    /// when nobody is close behind.
    pub throw_ahead: bool,
    /// How far ahead the gun is fired at a kart in line, metres, and the
    /// cone it must be in, degrees.
    pub gun_range: f32,
    pub gun_cone: f32,
    /// The rocket's lock: cone, degrees, and range, metres.
    pub rocket_cone: f32,
    pub rocket_range: f32,
    /// Seconds a shield is held before it goes up anyway.
    pub shield_hold: f32,
    /// The shield put up at once when a kart close behind carries a weapon
    /// it could fire forward.
    pub shield_react: bool,
    /// Overdrive kept for a straight, as the boost is.
    pub over_straight: bool,
}

impl Default for Brain {
    /// What five rounds of three brains bred from the original (notes/
    /// KART_BRAINS.md): it fires back and throws ahead, shields against an
    /// armed kart behind, drifts the tighter bends, rides slipstreams, and
    /// trusts 12% more grip and 6% more braking than the style fitted to its
    /// kart.
    fn default() -> Brain {
        Brain {
            corner: 1.12,
            brake: 1.06,
            drift: 0.016,
            draft: true,
            rear_fire: true,
            throw_ahead: true,
            shield_react: true,
            ..Brain::original()
        }
    }
}

impl Brain {
    /// The plain brain: the driver as `Style::for_kart` fits it, the weapons
    /// always fired forward and dropped behind.
    pub const fn original() -> Brain {
        Brain {
            corner: 1.0,
            brake: 1.0,
            look: 1.0,
            pass_look: 1.0,
            pass_side: 1.0,
            pads: true,
            boxes: 0.0,
            drift: 0.0,
            draft: false,
            rear_fire: false,
            throw_ahead: false,
            gun_range: 70.0,
            gun_cone: 6.0,
            rocket_cone: 25.0,
            rocket_range: 80.0,
            shield_hold: 4.0,
            shield_react: false,
            over_straight: false,
        }
    }

    /// The driving style for a kart: `base` with this brain's shares.
    pub fn style(&self, base: crate::vehicle::bot::Style) -> crate::vehicle::bot::Style {
        crate::vehicle::bot::Style {
            corner_g: base.corner_g * self.corner,
            brake_g: base.brake_g * self.brake,
            look_secs: base.look_secs * self.look,
            pass_look: base.pass_look * self.pass_look,
            pass_side: base.pass_side * self.pass_side,
            pads: self.pads,
            drift: self.drift,
            draft: self.draft,
            ..base
        }
    }
}

/// How fast a kart is against the field (1/3 of the rivals each way): a
/// share on its top speed, power and drive force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    Good,
    Normal,
    Bad,
}

impl Tier {
    pub fn share(self) -> f32 {
        match self {
            Tier::Good => 1.02,
            Tier::Normal => 1.0,
            Tier::Bad => 0.97,
        }
    }

    pub fn letter(self) -> char {
        match self {
            Tier::Good => 'G',
            Tier::Normal => 'N',
            Tier::Bad => 'B',
        }
    }

    fn from_letter(c: char) -> Option<Tier> {
        match c.to_ascii_uppercase() {
            'G' => Some(Tier::Good),
            'N' => Some(Tier::Normal),
            'B' => Some(Tier::Bad),
            _ => None,
        }
    }

    /// `n` rivals' tiers: a third good, a third bad, the rest normal, in an
    /// order shuffled by `seed`.
    pub fn deal(n: usize, seed: u64) -> Vec<Tier> {
        let third = (n as f32 / 3.0).round() as usize;
        let mut out: Vec<Tier> = (0..n).map(|i| if i < third { Tier::Good } else if i < 2 * third { Tier::Bad } else { Tier::Normal }).collect();
        let mut s = seed | 1;
        for i in (1..n).rev() {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            out.swap(i, (s % (i as u64 + 1)) as usize);
        }
        out
    }
}

/// A headless run's field, from `KART_BRAINS` (a JSON file): the brains by
/// name, which one each kart (0 the player's, driven by a bot) gets, and
/// optionally each kart's tier (`"GGNNBB"`).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LineupFile {
    brains: std::collections::BTreeMap<String, Brain>,
    karts: Vec<String>,
    #[serde(default)]
    tiers: Option<String>,
}

#[derive(Clone)]
pub struct Lineup {
    pub names: Vec<String>,
    pub brains: Vec<Brain>,
    pub tiers: Option<Vec<Tier>>,
}

impl Lineup {
    /// The field `KART_BRAINS` names, if it does. A brain called
    /// `original` or `default` that the file doesn't define is the built-in one.
    pub fn from_env() -> Option<Lineup> {
        let path = std::env::var("KART_BRAINS").ok()?;
        let text = std::fs::read_to_string(&path).map_err(|e| eprintln!("KART_BRAINS {path}: {e}")).ok()?;
        let f: LineupFile = serde_json::from_str(&text).map_err(|e| eprintln!("KART_BRAINS {path}: {e}")).ok()?;
        let brains = f
            .karts
            .iter()
            .map(|n| match (f.brains.get(n), n.as_str()) {
                (Some(b), _) => *b,
                (None, "original") => Brain::original(),
                (None, _) => {
                    if n != "default" {
                        eprintln!("KART_BRAINS: no brain called {n}; it drives as the default");
                    }
                    Brain::default()
                }
            })
            .collect();
        let tiers = f.tiers.map(|t| t.chars().filter_map(Tier::from_letter).collect());
        Some(Lineup { names: f.karts, brains, tiers })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_third_each_way() {
        for seed in 0..20 {
            let t = Tier::deal(6, seed);
            assert_eq!(t.iter().filter(|t| **t == Tier::Good).count(), 2);
            assert_eq!(t.iter().filter(|t| **t == Tier::Bad).count(), 2);
        }
        let t = Tier::deal(3, 7);
        assert_eq!(t.iter().filter(|t| **t == Tier::Normal).count(), 1);
    }
}
