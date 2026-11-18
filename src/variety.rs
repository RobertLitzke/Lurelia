//! Variation within a species: no two individuals quite alike. A creature's
//! `vary` block gives the spread; each individual rolls its own `Morph` once
//! from a seed and keeps it. Rare morphs (whole other library entries, with
//! `like` and `rarity`) are picked at spawn by `pick`.
//!
//! A named coat (`morphs` in the library) is a `Repaint`: one affine map over
//! every colour in the model, fitted from the colours the coat declares. It
//! rides the same per-instance tint, so a coat costs no palette slots and a
//! species can come in a dozen colourings for nothing.

use crate::anim::life::{Habits, ACTS};
use crate::creatures::age::{Stage, Stages};
use serde::Deserialize;

/// Spreads, each a fraction either side of the species' own value.
#[derive(Deserialize, Clone, Debug)]
pub struct Vary {
    /// Body size.
    #[serde(default = "d_size")]
    pub size: f32,
    /// Brightness of the coat.
    #[serde(default = "d_shade")]
    pub shade: f32,
    /// Warmer (redder) or cooler (bluer) coat.
    #[serde(default = "d_hue")]
    pub hue: f32,
    /// Walking and running speed.
    #[serde(default = "d_speed")]
    pub speed: f32,
    /// Distance at which it notices the player or a predator.
    #[serde(default = "d_wary")]
    pub wary: f32,
    /// Taste for each activity (life.rs weights): at 0.4 one deer may graze
    /// half again as often as another and lie down half as often.
    #[serde(default = "d_habits")]
    pub habits: f32,
}

fn d_size() -> f32 {
    0.06
}
fn d_shade() -> f32 {
    0.07
}
fn d_hue() -> f32 {
    0.04
}
fn d_speed() -> f32 {
    0.08
}
fn d_wary() -> f32 {
    0.2
}
fn d_habits() -> f32 {
    0.4
}

impl Default for Vary {
    fn default() -> Self {
        Self {
            size: d_size(),
            shade: d_shade(),
            hue: d_hue(),
            speed: d_speed(),
            wary: d_wary(),
            habits: d_habits(),
        }
    }
}

/// An affine map over every colour a model is drawn in: `colour * mul + add`,
/// in linear RGB. The multiply alone can darken a coat but not repaint it (no
/// multiple of a dark brown is a pale lilac), which is what `add` is for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Repaint {
    pub mul: [f32; 3],
    pub add: [f32; 3],
}

impl Repaint {
    /// The map that leaves a model as it was drawn.
    pub const PLAIN: Repaint = Repaint { mul: [1.0; 3], add: [0.0; 3] };

    /// Where this map sends one colour.
    pub fn of(&self, c: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| (c[i] * self.mul[i] + self.add[i]).max(0.0))
    }

    /// Partway from leaving it alone to this map, for the ends of a range.
    pub fn mix(a: &Repaint, b: &Repaint, t: f32) -> Repaint {
        Repaint {
            mul: [0, 1, 2].map(|i| a.mul[i] + (b.mul[i] - a.mul[i]) * t),
            add: [0, 1, 2].map(|i| a.add[i] + (b.add[i] - a.add[i]) * t),
        }
    }
}

/// How hard the fit is pulled toward a plain multiply. Small: it is there to
/// settle the one case the data cannot, a coat that names a single colour,
/// where "paint the fur this" has no second point to say how the rest of the
/// model should follow and a multiply is the honest reading.
const RIDGE: f32 = 1e-4;

/// Fit `colour * mul + add` to the colours a coat declares: pairs of (the
/// colour the model is drawn in, the colour the coat wants there), in linear
/// RGB. Least squares per channel, so two or more declared colours give a map
/// that carries the rest of the model with them, and one gives a multiply.
///
/// The fit is linear in the wanted colours, so fitting the midpoint of two
/// coats is the same as mixing their maps: `Repaint::mix` can walk a range.
pub fn fit(pairs: &[([f32; 3], [f32; 3])]) -> Repaint {
    let mut out = Repaint::PLAIN;
    if pairs.is_empty() {
        return out;
    }
    let n = pairs.len() as f32;
    for c in 0..3 {
        let sb: f32 = pairs.iter().map(|(b, _)| b[c]).sum();
        let sbb: f32 = pairs.iter().map(|(b, _)| b[c] * b[c]).sum();
        let st: f32 = pairs.iter().map(|(_, t)| t[c]).sum();
        let sbt: f32 = pairs.iter().map(|(b, t)| b[c] * t[c]).sum();
        // Normal equations for sum (m*b + a - t)^2 + RIDGE * a^2.
        let det = sbb * (n + RIDGE) - sb * sb;
        if det.abs() < 1e-12 {
            // Every declared colour is the same shade of grey: a multiply.
            out.mul[c] = if sb > 1e-6 { st / sb } else { 1.0 };
            out.add[c] = 0.0;
            continue;
        }
        out.mul[c] = (sbt * (n + RIDGE) - sb * st) / det;
        out.add[c] = (sbb * st - sb * sbt) / det;
    }
    out
}

/// A named coat ready to wear: its share of the population, how many shades
/// it comes in, and the maps for the two ends of its range. Built once, when
/// the world's creatures are compiled (worldload.rs).
#[derive(Clone, Debug)]
pub struct Wear {
    pub name: String,
    /// Share of individuals where nothing overrides it; the shares of a
    /// species' coats sum to 1.
    pub share: f32,
    pub shades: u32,
    /// The maps for the two ends of the coat's colour range; the same map
    /// twice for a coat that names one colour.
    pub ends: [Repaint; 2],
}

impl Wear {
    /// The map `at` of the way along its range.
    pub fn at(&self, t: f32) -> Repaint {
        Repaint::mix(&self.ends[0], &self.ends[1], t.clamp(0.0, 1.0))
    }
}

/// The coats of a creature, fitted from the colours its library entry is
/// drawn in: `colors` is slot -> sRGB, and each coat names the slots it
/// paints and the range it paints them (library::Coat).
pub fn wears(coats: &[crate::library::Coat], colors: &std::collections::HashMap<String, Vec<f32>>) -> Vec<Wear> {
    let lin = |c: &[f32]| -> [f32; 3] {
        let v = crate::models::linear_colour(c);
        [v[0], v[1], v[2]]
    };
    coats
        .iter()
        .map(|c| {
            let mut ends = [Vec::new(), Vec::new()];
            for (slot, paint) in &c.paint {
                let (Some(base), Some((lo, hi))) = (colors.get(slot), paint.ends()) else {
                    continue;
                };
                let b = lin(base);
                ends[0].push((b, lin(lo)));
                ends[1].push((b, lin(hi)));
            }
            Wear {
                name: c.name.clone(),
                share: c.share,
                shades: c.shades.max(1),
                ends: [fit(&ends[0]), fit(&ends[1])],
            }
        })
        .collect()
}

/// One individual's draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Morph {
    pub scale: f32,
    /// Multiplies the coat colour (the entity instance tint).
    pub tint: [f32; 3],
    /// Added after the multiply (the entity instance shift): the named coat
    /// this one wears, if any, worked into the same per-instance colour.
    pub shift: [f32; 3],
    pub speed: f32,
    pub wary: f32,
    /// Weight multipliers for each activity, in `anim::life::ACTS` order.
    pub habits: Habits,
    /// Head and ears drawn this much larger than the body's scale: a
    /// juvenile's head is big for its body.
    pub head: f32,
    /// How stiffly it moves: 1 as usual, less for an old animal (shorter
    /// strides, fewer ear flicks).
    pub supple: f32,
}

impl Morph {
    /// Wear a named coat: its map, then this individual's own lightening or
    /// warming over the top of it. Multiplying both halves of the map by the
    /// individual's tint is the same as tinting the colour the coat painted,
    /// so the two compose exactly.
    pub fn wear(&mut self, coat: &Repaint) {
        self.shift = [0, 1, 2].map(|i| coat.add[i] * self.tint[i]);
        self.tint = [0, 1, 2].map(|i| coat.mul[i] * self.tint[i]);
    }

    pub fn roll(v: &Vary, seed: u32) -> Self {
        let mut rng = crate::world::noise::Rng::new(seed | 1);
        let mut r = || rng.signed();
        let scale = 1.0 + v.size * r();
        let b = 1.0 + v.shade * r();
        let h = v.hue * r();
        let speed = 1.0 + v.speed * r();
        let wary = 1.0 + v.wary * r();
        let mut habits = [1.0; ACTS.len()];
        for x in &mut habits {
            *x = (v.habits * r()).exp();
        }
        Self {
            scale,
            tint: [b * (1.0 + h), b, b * (1.0 - h)],
            shift: [0.0; 3],
            speed,
            wary,
            habits,
            head: 1.0,
            supple: 1.0,
        }
    }

    /// Lay this individual's age over its own roll (creatures/age.rs). A
    /// juvenile is smaller with a head and ears big for it; an old one is
    /// greyer, slower and stiffer.
    pub fn age(&mut self, stage: Stage, st: &Stages, aged_tint: [f32; 3]) {
        match stage {
            Stage::Juvenile => {
                self.scale *= st.juvenile.max(0.1);
                self.head = JUVENILE_HEAD;
                // Young animals are quicker to startle and no faster.
                self.wary *= 1.15;
                self.speed *= 0.9;
            }
            Stage::Aged => {
                self.tint = [0, 1, 2].map(|i| self.tint[i] * aged_tint[i]);
                self.shift = [0, 1, 2].map(|i| self.shift[i] * aged_tint[i]);
                self.speed *= 0.8;
                self.wary *= 1.1;
                self.supple = 0.7;
            }
            // A body keeps the coat it had; bones are their own colours.
            Stage::Adult | Stage::Dead => {}
        }
    }
}

/// A juvenile's head and ears, relative to the rest of it.
pub const JUVENILE_HEAD: f32 = 1.15;
/// How far an old animal's coat goes toward grey, and how much it lightens.
const AGED_GREY: f32 = 0.62;
const AGED_LIFT: f32 = 1.12;

/// The tint that greys a species' coat, worked out once from the colours its
/// model uses (linear RGB). One tint multiplies every colour in the model at
/// once, so it cannot grey each of them exactly; aiming it at the mean coat
/// colour takes a brown or a red most of the way to grey and leaves an
/// already-grey coat alone but for the lightening.
pub fn aged_tint(coat: &[[f32; 3]]) -> [f32; 3] {
    if coat.is_empty() {
        return [1.0; 3];
    }
    let n = coat.len() as f32;
    let mean = [0, 1, 2].map(|i| (coat.iter().map(|c| c[i]).sum::<f32>() / n).max(1e-3));
    let lum = (mean[0] + mean[1] + mean[2]) / 3.0;
    // Where each channel should land, then what to multiply it by to get there.
    [0, 1, 2].map(|i| (((mean[i] + (lum - mean[i]) * AGED_GREY) * AGED_LIFT) / mean[i]).clamp(0.4, 2.5))
}

/// Speed and wariness multipliers for an individual; 1 before it has rolled.
pub fn pace(m: Option<&Morph>) -> (f32, f32) {
    m.map_or((1.0, 1.0), |m| (m.speed, m.wary))
}

/// Which of a species' coats an individual wears, and where in that coat's
/// range it falls: `shares` are the coats' shares of the population (they sum
/// to 1), or a biome's weights in their place. Two rolls in 0..1.
pub fn pick_coat(shares: &[f32], which: f32, step: f32, shades: impl Fn(usize) -> u32) -> Option<(usize, f32)> {
    let total: f32 = shares.iter().map(|s| s.max(0.0)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut acc = 0.0;
    let hit = shares.len() - 1;
    let hit = shares
        .iter()
        .position(|s| {
            acc += s.max(0.0) / total;
            which < acc
        })
        .unwrap_or(hit);
    // Where in the coat's range: a continuous draw at one shade, otherwise
    // one of `shades` steps, so a declared handful of shades stays a handful.
    let n = shades(hit);
    let at = if n <= 1 {
        step
    } else {
        let k = ((step * n as f32) as u32).min(n - 1);
        k as f32 / (n - 1) as f32
    };
    Some((hit, at))
}

/// A rare morph for one individual, given the parent's morphs as (index,
/// chance) and a roll in 0..1; None keeps the parent.
pub fn pick(morphs: &[(usize, f32)], roll: f32) -> Option<usize> {
    let mut acc = 0.0;
    for &(i, chance) in morphs {
        acc += chance;
        if roll < acc {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn individuals_differ_within_the_spread() {
        let v = Vary::default();
        let a = Morph::roll(&v, 1);
        let b = Morph::roll(&v, 2);
        assert_ne!(a, b);
        for s in 0..500 {
            let m = Morph::roll(&v, s);
            assert!((m.scale - 1.0).abs() <= v.size + 1e-6);
            assert!((m.tint[1] - 1.0).abs() <= v.shade + 1e-6);
            assert!(m.habits.iter().all(|h| (h.ln()).abs() <= v.habits + 1e-5));
        }
        let none = Vary {
            size: 0.0,
            shade: 0.0,
            hue: 0.0,
            speed: 0.0,
            wary: 0.0,
            habits: 0.0,
        };
        assert_eq!(
            Morph::roll(&none, 9),
            Morph {
                scale: 1.0,
                tint: [1.0; 3],
                shift: [0.0; 3],
                speed: 1.0,
                wary: 1.0,
                habits: [1.0; ACTS.len()],
                head: 1.0,
                supple: 1.0
            }
        );
    }

    #[test]
    fn a_juvenile_is_smaller_and_big_headed_and_an_old_one_greyer() {
        let st = Stages {
            juvenile: 0.62,
            aged: true,
            bones: true,
        };
        let brown = [[0.26, 0.17, 0.08]];
        let none = Vary {
            size: 0.0,
            shade: 0.0,
            hue: 0.0,
            speed: 0.0,
            wary: 0.0,
            habits: 0.0,
        };

        let mut juv = Morph::roll(&none, 3);
        juv.age(Stage::Juvenile, &st, aged_tint(&brown));
        assert!((juv.scale - 0.62).abs() < 1e-6);
        assert_eq!(juv.head, JUVENILE_HEAD);
        assert_eq!(juv.tint, [1.0; 3], "a juvenile keeps its coat");

        let mut old = Morph::roll(&none, 3);
        old.age(Stage::Aged, &st, aged_tint(&brown));
        assert_eq!(old.scale, 1.0);
        assert!(old.speed < 0.85 && old.supple < 1.0);
        // The brown's channels end up closer together than they started.
        let lit = |m: &Morph| [0, 1, 2].map(|i| brown[0][i] * m.tint[i]);
        let spread = |c: [f32; 3]| c.iter().cloned().fold(0.0f32, f32::max) - c.iter().cloned().fold(1.0f32, f32::min);
        assert!(spread(lit(&old)) < spread(brown[0]) * 0.75, "{:?}", lit(&old));

        let mut dead = Morph::roll(&none, 3);
        dead.age(Stage::Dead, &st, aged_tint(&brown));
        assert_eq!(dead.tint, [1.0; 3]);
        assert_eq!(dead.scale, 1.0);
    }

    #[test]
    fn greying_an_already_grey_coat_only_lightens_it() {
        let t = aged_tint(&[[0.2, 0.2, 0.2]]);
        assert!((t[0] - t[1]).abs() < 1e-6 && (t[1] - t[2]).abs() < 1e-6);
        assert!(t[0] > 1.0 && t[0] < 1.2, "{t:?}");
    }

    #[test]
    fn a_coat_that_names_two_colours_lands_on_them() {
        // A dark brown coat and its paler belly, repainted pale lilac.
        let pairs = [([0.26, 0.17, 0.08], [0.62, 0.58, 0.74]), ([0.46, 0.40, 0.30], [0.80, 0.78, 0.86])];
        let r = fit(&pairs);
        for (b, t) in pairs {
            let got = r.of(b);
            for i in 0..3 {
                assert!((got[i] - t[i]).abs() < 0.01, "{got:?} wanted {t:?}");
            }
        }
        // A multiply alone could not have done it: the brown's channels were
        // three apart and the lilac's are not.
        let spread = |c: [f32; 3]| c.iter().cloned().fold(0.0f32, f32::max) / c.iter().cloned().fold(1.0f32, f32::min);
        assert!(spread(r.of(pairs[0].0)) < 1.5 && spread(pairs[0].0) > 2.5);
    }

    #[test]
    fn a_coat_that_names_one_colour_only_multiplies() {
        // Nothing says how the rest of the model should follow, so the honest
        // reading is a multiply: it leaves black black and cannot blow out a
        // colour it was told nothing about.
        let r = fit(&[([0.4, 0.2, 0.1], [0.1, 0.05, 0.025])]);
        assert!(r.add.iter().all(|a| a.abs() < 1e-3), "{r:?}");
        for i in 0..3 {
            assert!((r.mul[i] - 0.25).abs() < 1e-3, "{r:?}");
        }
        assert_eq!(r.of([0.0; 3]), [0.0; 3]);
    }

    #[test]
    fn mixing_two_maps_is_fitting_the_middle() {
        // What lets a coat's range be walked by mixing the maps of its ends
        // rather than fitting afresh at every step.
        let base = [[0.26, 0.17, 0.08], [0.46, 0.40, 0.30]];
        let lo = [[0.52, 0.41, 0.77], [0.64, 0.53, 0.87]];
        let hi = [[0.69, 0.60, 0.92], [0.80, 0.74, 0.96]];
        let pairs = |t: &[[f32; 3]; 2]| [(base[0], t[0]), (base[1], t[1])];
        let middle: Vec<_> = (0..2).map(|k| (base[k], [0, 1, 2].map(|i| (lo[k][i] + hi[k][i]) * 0.5))).collect();
        let mixed = Repaint::mix(&fit(&pairs(&lo)), &fit(&pairs(&hi)), 0.5);
        let direct = fit(&middle);
        for i in 0..3 {
            assert!((mixed.mul[i] - direct.mul[i]).abs() < 1e-4 && (mixed.add[i] - direct.add[i]).abs() < 1e-4, "{mixed:?} {direct:?}");
        }
    }

    #[test]
    fn wearing_a_coat_keeps_the_individuals_own_draw() {
        // The coat paints, then this individual's lightening and warming goes
        // over the top: the same as tinting the colour the coat painted.
        let coat = fit(&[([0.26, 0.17, 0.08], [0.62, 0.58, 0.74]), ([0.46, 0.40, 0.30], [0.80, 0.78, 0.86])]);
        let mut m = Morph::roll(&Vary::default(), 7);
        let own = m.tint;
        m.wear(&coat);
        let base = [0.26, 0.17, 0.08];
        let drawn = [0, 1, 2].map(|i| base[i] * m.tint[i] + m.shift[i]);
        let wanted = [0, 1, 2].map(|i| coat.of(base)[i] * own[i]);
        for i in 0..3 {
            assert!((drawn[i] - wanted[i]).abs() < 1e-5, "{drawn:?} {wanted:?}");
        }
    }

    #[test]
    fn a_coat_is_picked_by_its_share_and_steps_through_its_shades() {
        let shares = [0.9, 0.1];
        let n = 20_000;
        let mut rare = 0;
        for k in 0..n {
            let which = crate::world::noise::unit(crate::world::noise::hash32(k));
            if pick_coat(&shares, which, 0.5, |_| 1) == Some((1, 0.5)) {
                rare += 1;
            }
        }
        assert!((rare as f32 / n as f32 - 0.1).abs() < 0.01, "{rare}");
        // One shade is a continuous draw; three are the ends and the middle.
        assert_eq!(pick_coat(&[1.0], 0.0, 0.37, |_| 1), Some((0, 0.37)));
        let steps: Vec<f32> = (0..9).map(|k| pick_coat(&[1.0], 0.0, k as f32 / 9.0, |_| 3).unwrap().1).collect();
        assert_eq!(steps, vec![0.0, 0.0, 0.0, 0.5, 0.5, 0.5, 1.0, 1.0, 1.0]);
        // Weights that are all zero leave the species in its own colours.
        assert_eq!(pick_coat(&[0.0, 0.0], 0.5, 0.5, |_| 1), None);
    }

    #[test]
    fn rare_morphs_turn_up_about_as_often_as_their_chance() {
        let morphs = [(7, 0.02), (8, 0.1)];
        let n = 100_000;
        let mut seen = [0; 2];
        for k in 0..n {
            let roll = crate::world::noise::unit(crate::world::noise::hash32(k));
            match pick(&morphs, roll) {
                Some(7) => seen[0] += 1,
                Some(8) => seen[1] += 1,
                _ => {}
            }
        }
        assert!((seen[0] as f32 / n as f32 - 0.02).abs() < 0.004, "{seen:?}");
        assert!((seen[1] as f32 / n as f32 - 0.1).abs() < 0.006, "{seen:?}");
    }
}
