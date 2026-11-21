//! Creatures: behaviour, ages, morphs and coats, and the checks on them.

#[allow(unused_imports)]
use super::*;

#[derive(Deserialize, Clone, Debug)]
pub struct Behaviour {
    /// Movement verb, one of creatures::locomotion::MOVEMENTS.
    pub movement: String,
    #[serde(default = "default_walk")]
    pub walk_speed: f32,
    #[serde(default = "default_flee")]
    pub flee_speed: f32,
    /// Distance at which the player (or a predator) is noticed, metres.
    #[serde(default = "default_wary")]
    pub wary: f32,
    /// How readily it bolts from the player once inside `wary`, where 0 is a
    /// manta ray that carries on as if nobody were there, about 0.2 a
    /// crocodile, 1 an ordinary wild animal and 2 a hare. `wary` is the flee
    /// distance; this is the likelihood of running (creatures::startle).
    #[serde(default = "one_f")]
    pub flightiness: f32,
    /// A juvenile's flightiness relative to an adult's: below 1 where the
    /// young stand and stare (fawns, bear cubs), above 1 where they are
    /// jumpier than their parents. The aged are always a little warier.
    #[serde(default = "one_f")]
    pub juvenile_flightiness: f32,
    /// Creature names (or "player") this one runs from.
    #[serde(default)]
    pub flees_from: Vec<String>,
    /// Creature names this one chases.
    #[serde(default)]
    pub hunts: Vec<String>,
    /// Walkers: crosses rivers too deep to wade by swimming (head up,
    /// carried by the current) instead of turning back at the water.
    #[serde(default)]
    pub swims: bool,
    /// Flying height above the ground or sea, metres (circle, flutter, flock, soar).
    #[serde(default)]
    pub fly_height: f32,
    /// Wingbeat pattern for rigged birds (anim/flight.rs): flap, flap_glide,
    /// bound, soar, hover. Defaults from the movement.
    #[serde(default)]
    pub flight: Option<String>,
    /// Wingbeats per second; defaults from the wingspan.
    #[serde(default)]
    pub wingbeat: Option<f32>,
    /// Moving as a group: a flock (movement "flock") or a family group of
    /// walkers. See flock.rs.
    #[serde(default)]
    pub group: Option<crate::creatures::flock::Group>,
    /// Seabirds: now and then come down off the circle and float on the sea.
    #[serde(default)]
    pub floats: bool,
    /// Floating seabirds that dive after fish (cormorants, puffins): how
    /// deep they go, metres. They swim a weaving way under for a quarter of
    /// a minute and come up a little further on.
    #[serde(default)]
    pub dive_depth: Option<f32>,
    /// How often it leaps clear of the water: for `school`, leaps per fish
    /// per minute; for `breach`, the chance that a dive ends in a breach.
    #[serde(default)]
    pub leap: f32,
    /// Chance, each half-second check while its biome wants one, that a
    /// missing one turns up. Below 1 makes a creature rare (whales).
    #[serde(default = "one_f")]
    pub spawn_chance: f32,
    /// Swimmers (`school`): cruise at the surface with the back and eyes out
    /// of the water instead of under it (crocodiles).
    #[serde(default)]
    pub awash: bool,
    /// Swimmers (`school`): the least water, metres, they keep to and are
    /// found in. Default 2 at sea and 1.2 in rivers; a marsh swimmer 0.6.
    #[serde(default)]
    pub shallows: Option<f32>,
    /// Swimmers (`school`): how it lives (creatures/swim.rs `Habit`): "open"
    /// (the default: a school wandering open water), "cloud" (a tight cloud
    /// over one coral head that drops into it when startled), "hover" (one
    /// or two hanging near a home, pausing, turning to look), "patrol" (long
    /// slow passes, minding nobody), "rest" (lying on the sand, gliding off
    /// when disturbed), "bait" (a bait ball that swirls and parts round the
    /// diver), "pulse" (a jellyfish drifting), "burrow" (garden eels risen
    /// out of the sand, sinking back as the diver comes) or "cruise" (one
    /// huge, slow, fearless animal you can hold on to).
    #[serde(default)]
    pub habit: Option<String>,
    /// Swimmers: the band of depth below the surface it keeps to, metres.
    #[serde(default)]
    pub depth: Option<[f32; 2]>,
    /// Swimmers: the band of height over the bed it keeps to, metres (reef
    /// fish keep over the coral); wins over `depth`.
    #[serde(default)]
    pub above_bed: Option<[f32; 2]>,
    /// Flashes of its glowing colours per second (fireflies); 0 glows steady.
    /// Each one keeps its own rhythm and is dark between flashes.
    #[serde(default)]
    pub blink: f32,
    /// Lies down on its side rather than upright, and tips over to get there
    /// (anim/ragdoll.rs). Dogs, cats and seals do; deer and cattle don't.
    #[serde(default)]
    pub sleeps_on_side: bool,
    /// Snakes: holds its ground and warns — coiled, head up, tail buzzing —
    /// while anything presses on it, instead of leaving (a rattlesnake).
    #[serde(default)]
    pub warns: bool,
    /// The part of the day it keeps: "diurnal" (the default), "nocturnal",
    /// "crepuscular" or "any". Outside those hours few of it are abroad and
    /// the ones that are, if they sleep where they stand, are bedded down
    /// (creatures/diel.rs).
    #[serde(default)]
    pub active: Option<String>,
    /// The share of it still abroad outside its hours, 0 to 1. Unset:
    /// `diel::OFF_HOURS`. 0 means it is only ever seen in its own hours (a
    /// firefly by day), and a higher number suits an animal that keeps loose
    /// hours (a bear).
    #[serde(default)]
    pub off_hours: Option<f32>,
    /// What it minds about the weather (world/weather.rs `Taste`): the share
    /// of it still abroad in a downpour, in falling snow, in the full heat of
    /// the day, and in a gale. Each is 0 (gone) to 1 (doesn't notice it), and
    /// each left out follows from the way it moves -- birds and insects keep
    /// out of rain and wind, walkers mind it less, fish not at all -- so most
    /// entries need none of them. A lizard sets `heat: 1`, a seabird
    /// `wind: 1`, a duck `rain: 1`.
    #[serde(default)]
    pub rain: Option<f32>,
    #[serde(default)]
    pub snow: Option<f32>,
    #[serde(default)]
    pub heat: Option<f32>,
    #[serde(default)]
    pub wind: Option<f32>,
    /// The share of it still about at the depth of winter, 0 to 1
    /// (world/season.rs). Unset, the way it moves decides: most of the
    /// walkers are still out, half the birds, almost none of the insects.
    /// A bear that sleeps through it sets 0; a bird that stays all year
    /// sets 1. Between winter and summer the year walks from this to all
    /// of it, so one number covers migrating and hibernating both.
    #[serde(default)]
    pub winter: Option<f32>,
}

impl Behaviour {
    /// The part of the day this creature keeps (creatures/diel.rs). An
    /// unknown name reads as diurnal; the library check catches it.
    pub fn period(&self) -> crate::creatures::diel::Period {
        self.active.as_deref().and_then(crate::creatures::diel::Period::parse).unwrap_or_default()
    }

    /// The share of it abroad outside its hours.
    pub fn off_hours(&self) -> f32 {
        self.off_hours.unwrap_or(crate::creatures::diel::OFF_HOURS)
    }
}

/// Which ages of a creature turn up, and what they look like. Every field is
/// optional: left out, it follows from the creature's `height`
/// (creatures::age::Stages::resolve), so most entries need no block at all.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct Ages {
    /// Body scale of a juvenile. 0 means this creature has no juvenile stage
    /// (starlings, insects, fish): every one spawns full-grown.
    #[serde(default)]
    pub juvenile: Option<f32>,
    /// Whether old individuals turn up, greyer and slower.
    #[serde(default)]
    pub aged: Option<bool>,
    /// "full": it leaves a skeleton when it dies. "none": nothing remains,
    /// because the bones are too small to read (sparrows, lizards, frogs).
    #[serde(default)]
    pub bones: Option<String>,
}

pub(super) fn one_f() -> f32 {
    1.0
}

pub(super) fn one_u() -> u32 {
    1
}

/// What a morph puts in one of an entry's colour slots: another material by
/// name (a plant's leaf, where the material already exists), or the range of
/// colour the slot is drawn in, as two ends in sRGB bytes with an optional
/// fourth emissive value. One colour is a range with both ends the same.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum Paint {
    /// A material by name. Plants only: a creature's colours are its own.
    Material(String),
    /// Two ends of a range.
    Range([Vec<f32>; 2]),
    /// One colour.
    One(Vec<f32>),
}

impl Paint {
    /// The two ends of the range, or None for a material by name.
    pub fn ends(&self) -> Option<(&[f32], &[f32])> {
        match self {
            Paint::Material(_) => None,
            Paint::Range([a, b]) => Some((a, b)),
            Paint::One(c) => Some((c, c)),
        }
    }
}

/// One named coat a species or plant comes in: the purple wisteria, the pale
/// hare, the white humpback. A morph says only what colour it changes, so
/// everything else about the individual is the entry's own. `shades` is how
/// many distinct draws it takes from inside its range, so one cluster of
/// purple wisteria is not quite the colour of the next.
///
/// A morph is not a species: it carries no shape, which is what separates it
/// from a `like` child. See docs/CREATURES.md, "Coats".
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct MorphDef {
    pub name: String,
    /// Relative, not a share: the entry's weights are normalised at load, so
    /// `9` and `1` give nine in ten and one in ten.
    #[serde(default = "one_f")]
    pub weight: f32,
    /// How many distinct draws the coat is built from. 1, the default, is a
    /// continuous draw where the renderer can manage one and a single colour
    /// where it cannot: a creature's coat is a per-instance tint and costs
    /// nothing, while a plant's is a material and each shade is an atlas
    /// layer, so ask for more only where it will be seen.
    #[serde(default = "one_u")]
    pub shades: u32,
    /// Slot name -> what fills it. A creature's slots are the keys of its own
    /// `colors`; a plant's are bark, leaf, flower, autumn and stalk.
    #[serde(default)]
    pub colors: HashMap<String, Paint>,
    #[serde(default)]
    #[allow(dead_code)] // read by people
    pub notes: String,
}

/// The name of the material generated for one slot of a plant's coat. One
/// per plant, slot, coat and shade, so it is unique however many plants share
/// a base material or a coat name.
pub fn coat_material(plant: &str, slot: &str, coat: &str, shade: u32, shades: u32) -> String {
    if shades <= 1 {
        format!("{plant}_{slot}@{coat}")
    } else {
        format!("{plant}_{slot}@{coat}.{shade}")
    }
}

/// The materials a plant wears in the coat it drew: slot -> material, in
/// `PLANT_SLOTS` order, and None for a slot the coat leaves alone.
#[derive(Clone, Copy, Default, Debug)]
pub struct Worn(pub(super) [Option<u8>; PLANT_SLOTS.len()]);

impl Worn {
    pub fn get(&self, slot: &str) -> Option<u8> {
        PLANT_SLOTS.iter().position(|s| *s == slot).and_then(|i| self.0[i])
    }
}

/// Carry a recipe's colours to a new one: the multiply that takes the
/// recipe's own base colour to `want` is applied to every colour in it, so a
/// blotched lilac repainted white stays blotched.
pub(super) fn repaint_recipe(r: &mut Recipe, want: &[f32]) {
    if want.len() < 3 {
        return;
    }
    let mul = [0, 1, 2].map(|i| if r.base[i] > 1.0 { (want[i] / r.base[i]).clamp(0.0, 8.0) } else { 1.0 });
    let put = |c: &mut [f32; 3]| *c = [0, 1, 2].map(|i| (c[i] * mul[i]).clamp(0.0, 255.0));
    put(&mut r.base);
    // The base lands exactly where the coat asked, whatever the multiply did.
    r.base = [want[0], want[1], want[2]];
    if let Some(a) = r.alt.as_mut() {
        put(a);
    }
    if let Some(c) = r.calm.as_mut() {
        put(&mut c.base);
        if let Some(a) = c.alt.as_mut() {
            put(a);
        }
    }
}

/// The slots a plant's morph may paint.
pub const PLANT_SLOTS: [&str; 5] = ["bark", "leaf", "flower", "autumn", "stalk"];

/// A morph with its weights worked out: what share of individuals wear it.
#[derive(Clone, Debug)]
pub struct Coat {
    pub name: String,
    /// Share of individuals, the entry's weights normalised to 1.
    pub share: f32,
    pub shades: u32,
    /// Slot -> what fills it, in a fixed order so a build is reproducible.
    pub paint: Vec<(String, Paint)>,
}

/// Morph defs to coats, weights normalised. An entry with no morphs has no
/// coats at all rather than one default coat, so nothing downstream has to
/// treat "one coat" as a special case of plain.
/// The coats an entry comes in, for an entry that is not in the library as it
/// stands: the EDIT page's own copy of one (src/editor).
pub fn coats_of(morphs: &[MorphDef]) -> Vec<Coat> {
    coats(morphs)
}

pub(super) fn coats(morphs: &[MorphDef]) -> Vec<Coat> {
    let total: f32 = morphs.iter().map(|m| m.weight.max(0.0)).sum();
    if morphs.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    morphs
        .iter()
        .map(|m| {
            let mut paint: Vec<(String, Paint)> = m.colors.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            paint.sort_by(|a, b| a.0.cmp(&b.0));
            Coat {
                name: m.name.clone(),
                share: m.weight.max(0.0) / total,
                shades: m.shades.max(1),
                paint,
            }
        })
        .collect()
}

pub(super) fn default_walk() -> f32 {
    1.2
}
pub(super) fn default_flee() -> f32 {
    6.0
}
pub(super) fn default_wary() -> f32 {
    8.0
}

#[derive(Deserialize, Clone, Debug)]
pub struct CreatureDef {
    pub name: String,
    #[serde(default = "default_version")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    /// Model voxel edge, metres (the model grid is 32 voxels on a side).
    pub voxel: f32,
    pub half_w: f32,
    pub height: f32,
    /// Colour name -> sRGB, with optional emissive strength as a fourth value.
    pub colors: HashMap<String, Vec<f32>>,
    pub ops: Vec<Op>,
    pub behaviour: Behaviour,
    /// How it moves once it is in the water, for a species that crosses
    /// between the two: an alligator on the bank and awash among the knees, a
    /// seal hauled out and at sea. The engine builds a second form of the
    /// species from this (`worldload::creatures`), so one entry and one name
    /// covers both and a biome lists it once. Merged key by key over
    /// `behaviour`, so it need only give what differs.
    #[serde(default)]
    pub in_water: Option<Behaviour>,
    /// Bones for animation (ANIMATION.md); ops tagged `part` name the bone.
    #[serde(default)]
    pub rig: Option<crate::anim::RigDef>,
    /// The creature this one is a subspecies or morph of (src/inherit.rs);
    /// already merged in.
    #[serde(default)]
    pub like: Option<String>,
    /// Above 0: a rare morph of its parent. Where the parent spawns, this
    /// fraction of individuals are this creature instead. `rarity` makes a
    /// rare *species*, which may differ in any way including its shape (the
    /// unicorn's horn); `morphs` below are coats of one species.
    #[serde(default)]
    pub rarity: f32,
    /// The coats this species comes in (see `MorphDef`): the pale hare, the
    /// white humpback, the albino fox. Never inherited through `like`, since a
    /// morph's colours are absolute and a child is a different animal.
    #[serde(default)]
    pub morphs: Vec<MorphDef>,
    /// How much individuals differ (src/variety.rs).
    #[serde(default)]
    pub vary: crate::variety::Vary,
    /// Which ages turn up (src/creatures/age.rs); defaults from `height`.
    #[serde(default)]
    pub ages: Ages,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
}

/// An entry's coats: each must name itself once, weigh something, and paint
/// only slots the entry has. `mats` says whether a slot may be filled by
/// naming another material (plants can, since their colour is a material; a
/// creature's colours are its own).
pub(super) fn check_morphs(morphs: &[MorphDef], slots: &[String], mats: bool, what: &str, lib: &Library, out: &mut Vec<String>) {
    if morphs.is_empty() {
        return;
    }
    let mut seen: Vec<&str> = Vec::new();
    for m in morphs {
        if m.name.is_empty() {
            out.push(format!("{what}: a morph with no name"));
        }
        if seen.contains(&m.name.as_str()) {
            out.push(format!("{what}: two morphs called {}", m.name));
        }
        seen.push(&m.name);
        if !(m.weight > 0.0) {
            out.push(format!("{what}: morph {}: weight must be above 0", m.name));
        }
        if !(1..=8).contains(&m.shades) {
            out.push(format!("{what}: morph {}: shades must be 1 to 8", m.name));
        }
        for (slot, paint) in &m.colors {
            if !slots.iter().any(|s| s == slot) {
                out.push(format!("{what}: morph {}: {slot} is not one of its colours ({})", m.name, slots.join(", ")));
            }
            match paint {
                Paint::Material(n) => {
                    if !mats {
                        out.push(format!("{what}: morph {}: {slot} must be a colour, not the material {n}", m.name));
                    } else if lib.material(n).is_none() && !lib.materials.iter().any(|d| &d.name == n) {
                        out.push(format!("{what}: morph {}: unknown material {n}", m.name));
                    }
                }
                _ => {
                    let (a, b) = paint.ends().unwrap();
                    for c in [a, b] {
                        if !(3..=4).contains(&c.len()) {
                            out.push(format!("{what}: morph {}: {slot} wants 3 or 4 numbers, not {}", m.name, c.len()));
                        } else if c[..3].iter().any(|v| !(0.0..=255.0).contains(v)) {
                            out.push(format!("{what}: morph {}: {slot} is outside 0..255", m.name));
                        }
                    }
                    if a.len() != b.len() {
                        out.push(format!("{what}: morph {}: {slot}: one end glows and the other does not", m.name));
                    }
                }
            }
        }
    }
    if morphs.len() == 1 && morphs[0].colors.is_empty() {
        out.push(format!("{what}: one morph that paints nothing does nothing"));
    }
}

/// The coats as they come out: rare enough to never be met, or so close to
/// each other that nobody could tell them apart, and both are mistakes worth
/// catching before the world is built.
pub(super) fn check_coats(coats: &[Coat], what: &str, out: &mut Vec<String>) {
    for c in coats {
        if c.share < MIN_SHARE {
            out.push(format!("{what}: morph {}: one in {:.0} is too rare to ever be met", c.name, 1.0 / c.share.max(1e-9)));
        }
        // Shades are taken along the range, so a wide `shades` over a narrow
        // range asks for steps nobody could see.
        for (_, paint) in &c.paint {
            let Some((a, b)) = paint.ends() else { continue };
            // A malformed colour is `check_morphs`'s to report; say nothing
            // about it twice and do not reach past its end.
            if a.len() < 3 || b.len() < 3 {
                continue;
            }
            let d = [0, 1, 2].map(|i| a[i] - b[i]);
            let span = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if c.shades > 1 && span / ((c.shades - 1) as f32) < MIN_STEP {
                out.push(format!("{what}: morph {}: {} shades over a range {span:.0} wide are steps of under {MIN_STEP}", c.name, c.shades));
            }
        }
    }
    // The middle of each range, per slot, so two coats are compared as the
    // player would see them rather than end against end.
    let mid = |c: &Coat, slot: &str| -> Option<[f32; 3]> {
        c.paint
            .iter()
            .find(|(s, _)| s == slot)
            .and_then(|(_, p)| p.ends())
            .filter(|(a, b)| a.len() >= 3 && b.len() >= 3)
            .map(|(a, b)| [0, 1, 2].map(|i| (a[i] + b[i]) * 0.5))
    };
    for (i, a) in coats.iter().enumerate() {
        for b in &coats[i + 1..] {
            let slots: Vec<&str> = a.paint.iter().map(|(s, _)| s.as_str()).filter(|s| b.paint.iter().any(|(t, _)| t == s)).collect();
            if slots.is_empty() {
                continue;
            }
            let apart = slots.iter().filter_map(|s| Some((mid(a, s)?, mid(b, s)?))).map(|(x, y)| {
                let d = [0, 1, 2].map(|i| x[i] - y[i]);
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            });
            // Two coats are told apart if any one slot differs enough, so it
            // is the widest gap that matters.
            let worst = apart.fold(f32::NEG_INFINITY, f32::max);
            if worst.is_finite() && worst < MIN_APART {
                out.push(format!("{what}: morphs {} and {} are the same colour to look at", a.name, b.name));
            }
        }
    }
}

/// Where a creature's coats actually land. A coat is one affine map over
/// every colour in the model (variety::Repaint), fitted from the colours it
/// names, so a coat naming one colour out of six can only stretch the other
/// five to fit. Worth reporting when that stretch drives a colour past white,
/// which is where it stops looking like a coat and starts looking broken.
pub(super) fn check_fit(coats: &[Coat], colors: &HashMap<String, Vec<f32>>, what: &str, out: &mut Vec<String>) {
    let lin = |c: &[f32]| {
        let v = crate::models::linear_colour(c);
        [v[0], v[1], v[2]]
    };
    // Back to sRGB bytes, so a miss is reported in the numbers the morph was
    // written in.
    let srgb = |c: [f32; 3]| c.map(|v| v.max(0.0).powf(1.0 / 2.2) * 255.0);
    let apart = |a: [f32; 3], b: [f32; 3]| {
        let d = [0, 1, 2].map(|i| a[i] - b[i]);
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    };
    for (coat, wear) in coats.iter().zip(crate::variety::wears(coats, colors)) {
        // Can the coat reach the colours it asked for? One tint repaints the
        // whole model at once, so it can take every colour lighter, darker or
        // warmer together, but it cannot send two of them different ways: a
        // blue body to red and a green thorax to brown is not one tint.
        for (slot, paint) in &coat.paint {
            let (Some(base), Some((a, b))) = (colors.get(slot), paint.ends()) else {
                continue;
            };
            if base.len() < 3 || a.len() < 3 || b.len() < 3 {
                continue;
            }
            for (t, want) in [(0.0f32, a), (1.0, b)] {
                let miss = apart(srgb(wear.at(t).of(lin(base))), srgb(lin(want)));
                if miss > REACH {
                    out.push(format!(
                        "{what}: morph {} cannot reach {slot}: one tint repaints a whole model, so it lands {miss:.0} away at {}",
                        coat.name,
                        srgb(wear.at(t).of(lin(base))).map(|v| v.round() as i32).iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ")
                    ));
                    break;
                }
            }
        }
        // And where does it leave the colours it said nothing about?
        for (slot, rgb) in colors {
            if rgb.len() < 3 || coat.paint.iter().any(|(s, _)| s == slot) {
                continue;
            }
            let worst = [0.0f32, 1.0].into_iter().flat_map(|t| wear.at(t).of(lin(rgb))).fold(0.0f32, f32::max);
            if worst > BLOWN {
                out.push(format!(
                    "{what}: morph {} sends {slot} past white ({worst:.1} of it): it names {} of its {} colours, so name more of them or paint a nearer colour",
                    coat.name,
                    coat.paint.len(),
                    colors.len()
                ));
            }
        }
    }
}

/// How far past white a coat may send one of a model's colours before it is a
/// mistake rather than a bright coat.
pub(super) const BLOWN: f32 = 1.35;
/// How near a coat must land to the colour it declared, in sRGB. Generous: a
/// few counts off is invisible, and the point is to catch a coat asking for
/// something one tint cannot do at all.
pub(super) const REACH: f32 = 24.0;

/// The rarest coat worth declaring: below about one in five hundred an
/// individual wearing it is never met, and the morph is dead weight.
pub(super) const MIN_SHARE: f32 = 0.002;
/// How far apart two coats must be in sRGB to read as different colours.
pub(super) const MIN_APART: f32 = 12.0;
/// The smallest step between shades of one coat worth drawing separately.
pub(super) const MIN_STEP: f32 = 3.0;

/// A biome asking for coats must name ones the entry has, and must not ask at
/// all of an entry that comes in only one colouring.
pub(super) fn check_pick(pick: &Option<MorphPick>, coats: &[Coat], entry: &str, what: &str, out: &mut Vec<String>) {
    let Some(pick) = pick else { return };
    if coats.is_empty() {
        out.push(format!("{what}: {entry} comes in one colouring, so it has no morph to pick"));
        return;
    }
    for n in pick.names() {
        if !coats.iter().any(|c| c.name == n) {
            let have: Vec<&str> = coats.iter().map(|c| c.name.as_str()).collect();
            out.push(format!("{what}: {entry} has no morph {n} (it has {})", have.join(", ")));
        }
    }
    if let MorphPick::Weights(w) = pick {
        if w.is_empty() || w.values().all(|v| *v <= 0.0) {
            out.push(format!("{what}: {entry}: the morph weights are all zero"));
        }
        if let Some((n, _)) = w.iter().find(|(_, v)| **v < 0.0) {
            out.push(format!("{what}: {entry}: morph {n} has a negative weight"));
        }
    }
}

/// Every colour an op names must be in the entry's palette, or the op is
/// silently skipped when the model is built.
pub(super) fn op_colours(ops: &[Op], colors: &HashMap<String, Vec<f32>>, what: &str, out: &mut Vec<String>) {
    for op in ops {
        let names: Vec<&String> = match op {
            Op::Box { mat, .. }
            | Op::Walls { mat, .. }
            | Op::Gable { mat, .. }
            | Op::Ellipsoid { mat, .. }
            | Op::Cylinder { mat, .. }
            | Op::Log { mat, .. }
            | Op::Disc { mat, .. }
            | Op::Dish { mat, .. }
            | Op::Segment { mat, .. }
            | Op::Set { mat, .. } => vec![mat],
            Op::RecolourBelow { from, to, .. } | Op::Recolour { from, to, .. } => vec![from, to],
            Op::Repeat { ops, .. } => {
                op_colours(ops, colors, what, out);
                vec![]
            }
        };
        for n in names {
            if !colors.contains_key(n) && n != "air" {
                let msg = format!("{what}: op colour {n} is not in its colors");
                if !out.contains(&msg) {
                    out.push(msg);
                }
            }
        }
    }
}
