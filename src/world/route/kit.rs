//! Route kits: a cross-section and what repeats along it.
//!
//! One kit file says how wide a line's surface is and what it is made of,
//! what rails run along it, what posts repeat, whether it rides on a deck and
//! what holds that deck up. A fence, a lane, a highway with steel guards, a
//! boardwalk and a rope span are all kits; nothing in the engine knows which
//! is which. `library/kits/*.json` holds them, loaded like every other kind.

use crate::library::Library;
use serde::Deserialize;

/// Which side of the line a member sits on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Left,
    Right,
    Both,
    Centre,
}

impl Side {
    fn read(s: &str) -> Side {
        match s {
            "left" => Side::Left,
            "right" => Side::Right,
            "centre" | "center" | "middle" => Side::Centre,
            _ => Side::Both,
        }
    }
    /// Does a signed offset across the line (positive to the left) count?
    #[inline]
    pub fn has(self, across: f32) -> bool {
        match self {
            Side::Both | Side::Centre => true,
            Side::Left => across >= 0.0,
            Side::Right => across <= 0.0,
        }
    }
}

/// What the line does to the land under it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Ground {
    /// Cut and fill the land to an even grade (roads, lanes).
    Grade,
    /// A free-standing deck `lift` m over whatever is below (boardwalks,
    /// skywalks); the land is left alone.
    Deck,
    /// Leave the land alone and sit the members on it (fences, kerb lines).
    Follow,
    /// Leave the land alone and draw nothing on it (a bare routed line).
    None,
    /// Hollow a passage out of the rock at the line's own height, with a
    /// floor of the surface material: a cave network, an adit, an underpass.
    Bore,
}

// ---- the file --------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub struct KitDef {
    pub name: String,
    #[serde(default = "one_u")]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub version: u32,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub introduced_by: String,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py and people
    pub notes: String,
    #[serde(default)]
    #[allow(dead_code)] // read by tools/libfmt.py
    pub tags: Vec<String>,
    /// grade, deck, follow or none.
    #[serde(default = "grade")]
    pub ground: String,
    /// Width of the surface, metres. 0 for a bare line (a fence).
    #[serde(default)]
    pub width: f32,
    /// Surface material; none leaves the land's own.
    #[serde(default)]
    pub mat: Option<String>,
    /// Material for the last metre each side of the surface.
    #[serde(default)]
    pub shoulder: Option<String>,
    /// How far beside the surface a graded cut is feathered back, metres.
    #[serde(default = "four")]
    pub margin: f32,
    #[serde(default)]
    pub walk: Option<WalkDef>,
    #[serde(default)]
    pub deck: Option<DeckDef>,
    #[serde(default)]
    pub rails: Vec<RailDef>,
    #[serde(default)]
    pub posts: Vec<PostDef>,
    #[serde(default)]
    pub piers: Option<PierDef>,
    /// How far the deck sags at mid-span, metres (rope bridges).
    #[serde(default)]
    pub sag: f32,
    /// The kit for stretches that leave the ground; none keeps this one.
    #[serde(default)]
    pub span: Option<String>,
    /// Air carved above the surface, metres: a tunnel's headroom.
    #[serde(default)]
    pub clear: f32,
    /// How high a `deck` line rides over the ground under it, metres.
    #[serde(default = "two")]
    pub lift: f32,
    /// How far a span may reach between supports before the router gives up
    /// and looks for another way round, metres.
    #[serde(default = "eighty")]
    pub max_span: f32,
}

/// A pavement each side, past a planting gap.
#[derive(Deserialize, Clone, Debug)]
pub struct WalkDef {
    pub width: f32,
    #[serde(default)]
    pub gap: f32,
    pub mat: String,
    /// A raised lip between the surface and the gap.
    #[serde(default)]
    pub kerb: Option<String>,
    #[serde(default = "both")]
    pub side: String,
}

/// The slab a line rides on where it leaves the ground.
#[derive(Deserialize, Clone, Debug)]
pub struct DeckDef {
    /// How thick the slab is under the walking surface, metres.
    #[serde(default = "quarter")]
    pub thick: f32,
    /// The top voxel of the deck (what you walk on).
    pub mat: String,
    /// The rest of the slab; the top material when left out.
    #[serde(default)]
    pub under: Option<String>,
}

/// A member running the whole length: a guard rail, a kerb, a rope.
#[derive(Deserialize, Clone, Debug)]
pub struct RailDef {
    #[serde(default = "both")]
    pub side: String,
    /// Metres outward from the surface edge; negative pulls it in.
    #[serde(default)]
    pub out: f32,
    /// Height above the surface, metres.
    pub from: f32,
    pub to: f32,
    #[serde(default = "quarter")]
    pub thick: f32,
    pub mat: String,
}

/// A member repeated along the line: a fence post, a lamp, a pier cap.
#[derive(Deserialize, Clone, Debug)]
pub struct PostDef {
    #[serde(default = "both")]
    pub side: String,
    #[serde(default)]
    pub out: f32,
    /// Metres between them, measured along the whole route.
    pub every: f32,
    /// Along, up, across: metres.
    pub size: [f32; 3],
    /// Metres along the route before the first one.
    #[serde(default)]
    pub phase: f32,
    /// Height above the surface the post starts at.
    #[serde(default)]
    pub base: f32,
    pub mat: String,
}

/// What holds a deck up where it stands clear of the ground.
#[derive(Deserialize, Clone, Debug)]
pub struct PierDef {
    pub every: f32,
    pub radius: f32,
    pub mat: String,
    /// Only where the deck stands at least this far above the ground.
    #[serde(default = "one_f")]
    pub min_drop: f32,
}

fn one_u() -> u32 {
    1
}
fn one_f() -> f32 {
    1.0
}
fn two() -> f32 {
    2.0
}
fn four() -> f32 {
    4.0
}
fn eighty() -> f32 {
    80.0
}
fn quarter() -> f32 {
    0.25
}
fn grade() -> String {
    "grade".into()
}
fn both() -> String {
    "both".into()
}

// ---- the kit as the generator uses it --------------------------------------

#[derive(Clone, Debug)]
pub struct Rail {
    pub side: Side,
    pub out: f32,
    pub from: f32,
    pub to: f32,
    pub thick: f32,
    pub mat: u8,
}

#[derive(Clone, Debug)]
pub struct Post {
    pub side: Side,
    pub out: f32,
    pub every: f32,
    pub along: f32,
    pub up: f32,
    pub across: f32,
    pub phase: f32,
    pub base: f32,
    pub mat: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Piers {
    pub every: f32,
    pub radius: f32,
    pub mat: u8,
    pub min_drop: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Walk {
    pub width: f32,
    pub gap: f32,
    pub mat: u8,
    pub kerb: u8,
    pub side: Side,
}

/// A kit with every name resolved to a material id, and its bounds worked out
/// once so the chunk generator can skip a column cheaply.
#[derive(Clone, Debug)]
pub struct Kit {
    #[allow(dead_code)] // for messages and tools
    pub name: String,
    pub ground: Ground,
    /// Half the surface width.
    pub half: f32,
    pub mat: u8,
    pub shoulder: u8,
    pub margin: f32,
    pub walk: Option<Walk>,
    pub deck_thick: f32,
    pub deck_mat: u8,
    pub deck_under: u8,
    pub rails: Vec<Rail>,
    pub posts: Vec<Post>,
    pub piers: Option<Piers>,
    pub sag: f32,
    /// Index into the world's kit list for airborne stretches.
    pub span: Option<usize>,
    pub clear: f32,
    pub lift: f32,
    pub max_span: f32,
    /// Widest the kit reaches either side of the line, metres.
    pub reach: f32,
    /// Highest a member stands above the surface, metres.
    pub rise: f32,
    /// How far the kit reaches below the surface, metres.
    pub drop: f32,
}

impl Kit {
    /// Resolve a kit file's names. Unknown materials are reported and skipped.
    pub fn build(def: &KitDef, lib: &Library, warn: &mut dyn FnMut(String)) -> Kit {
        let mut missing: Vec<String> = Vec::new();
        let mut mat = |n: &str| -> u8 {
            lib.material(n).unwrap_or_else(|| {
                missing.push(n.to_string());
                0
            })
        };
        let half = def.width * 0.5;
        let surface = def.mat.as_deref().map_or(0, &mut mat);
        let rails: Vec<Rail> = def
            .rails
            .iter()
            .map(|r| Rail {
                side: Side::read(&r.side),
                out: r.out,
                from: r.from,
                to: r.to,
                thick: r.thick.max(0.25),
                mat: mat(&r.mat),
            })
            .collect();
        let posts: Vec<Post> = def
            .posts
            .iter()
            .map(|p| Post {
                side: Side::read(&p.side),
                out: p.out,
                every: p.every.max(0.5),
                along: p.size[0].max(0.25),
                up: p.size[1].max(0.25),
                across: p.size[2].max(0.25),
                phase: p.phase,
                base: p.base,
                mat: mat(&p.mat),
            })
            .collect();
        let shoulder = def.shoulder.as_deref().map_or(surface, &mut mat);
        let walk = def.walk.as_ref().map(|w| Walk {
            width: w.width,
            gap: w.gap,
            mat: mat(&w.mat),
            kerb: w.kerb.as_deref().map_or(0, &mut mat),
            side: Side::read(&w.side),
        });
        let (deck_thick, deck_mat, deck_under) = match &def.deck {
            Some(d) => {
                let top = mat(&d.mat);
                (d.thick.max(0.25), top, d.under.as_deref().map_or(top, &mut mat))
            }
            // A line with no deck of its own still needs one where it flies.
            None => (0.5, surface, surface),
        };
        let piers = def.piers.as_ref().map(|p| Piers {
            every: p.every.max(1.0),
            radius: p.radius,
            mat: mat(&p.mat),
            min_drop: p.min_drop,
        });
        drop(mat);
        for n in missing {
            warn(format!("kit {}: unknown material {n}", def.name));
        }
        let ground = match def.ground.as_str() {
            "deck" => Ground::Deck,
            "follow" => Ground::Follow,
            "none" => Ground::None,
            "bore" => Ground::Bore,
            "grade" => Ground::Grade,
            other => {
                warn(format!("kit {}: unknown ground {other}, taking grade", def.name));
                Ground::Grade
            }
        };
        // How far out and how high anything reaches.
        let mut reach: f32 = half + walk.map_or(0.0, |w| w.gap + w.width);
        let mut rise: f32 = 0.0;
        for r in &rails {
            reach = reach.max(half + r.out + r.thick);
            rise = rise.max(r.to);
        }
        for p in &posts {
            reach = reach.max(half + p.out + p.across);
            rise = rise.max(p.base + p.up);
        }
        if let Some(p) = &def.piers {
            reach = reach.max(p.radius);
        }
        Kit {
            name: def.name.clone(),
            ground,
            half,
            mat: surface,
            shoulder,
            margin: def.margin,
            walk,
            deck_thick,
            deck_mat,
            deck_under,
            rails,
            posts,
            piers,
            sag: def.sag,
            span: None,
            clear: def.clear,
            lift: def.lift,
            max_span: def.max_span,
            reach: reach + 0.3,
            rise,
            drop: deck_thick,
        }
    }

    /// The material at a point in the kit's own frame, or none for air.
    /// `across` is metres left of the line (negative to the right), `up`
    /// metres above its surface and `s` metres along the whole route.
    /// `decked` says the surface is a deck in the air rather than graded land.
    pub fn voxel(&self, across: f32, up: f32, s: f32, decked: bool) -> Option<u8> {
        let a = across.abs();
        // A bored passage: a floor under the hollow, nothing else.
        if self.ground == Ground::Bore {
            if a <= self.half && up <= 0.0 && up > -self.deck_thick {
                return (self.mat != 0).then_some(self.mat);
            }
            return None;
        }
        // The deck slab. Graded land already holds the surface, so only a
        // flying stretch fills it in.
        if decked && a <= self.half && up <= 0.0 && up > -self.deck_thick {
            return Some(if up > -0.3 { self.deck_mat } else { self.deck_under });
        }
        // Pavements and their kerb.
        if let Some(w) = self.walk {
            if w.side.has(across) {
                if w.kerb != 0 && a >= self.half && a < self.half + w.gap.min(0.5).max(0.25) && up < 0.2 && up >= -0.25 {
                    return Some(w.kerb);
                }
                if a >= self.half + w.gap && a <= self.half + w.gap + w.width && up < 0.15 && up >= -0.25 {
                    return Some(w.mat);
                }
            }
        }
        for r in &self.rails {
            if !r.side.has(across) || up < r.from || up >= r.to {
                continue;
            }
            let (lo, hi) = band(self.half, r.out, r.thick, r.side);
            if a >= lo && a < hi {
                return Some(r.mat);
            }
        }
        for p in &self.posts {
            if !p.side.has(across) || up < p.base || up >= p.base + p.up {
                continue;
            }
            let (lo, hi) = band(self.half, p.out, p.across, p.side);
            if a < lo || a >= hi {
                continue;
            }
            if (s - p.phase).rem_euclid(p.every) < p.along {
                return Some(p.mat);
            }
        }
        None
    }
}

/// Where a member sits across the line: a centred one straddles it, the rest
/// stand `out` m beyond the surface edge.
fn band(half: f32, out: f32, thick: f32, side: Side) -> (f32, f32) {
    if side == Side::Centre {
        (0.0, thick * 0.5)
    } else {
        let lo = (half + out).max(0.0);
        (lo, lo + thick)
    }
}
