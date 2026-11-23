//! Route kits: a cross-section and what repeats along it.
//!
//! One kit file says how wide a line's surface is and what it is made of,
//! what rails run along it, what posts repeat, whether it rides on a deck and
//! what holds that deck up. A fence, a lane, a highway with steel guards, a
//! boardwalk and a rope span are all kits; nothing in the engine knows which
//! is which. `library/kits/*.json` holds them, loaded like every other kind.

use crate::library::Library;
use crate::world::gen::VOXEL;
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

#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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
    #[serde(default)]
    pub braces: Vec<BraceDef>,
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
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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
    /// 0 for a smooth bar. Above 0 the rail is a heap of stones: laid in
    /// courses of separate stones, narrowing upward, its top ragged by up to
    /// this fraction of its height; a field wall rather than a kerb.
    #[serde(default)]
    pub rubble: f32,
    /// A second stone, every other one or so, for a rubble rail.
    #[serde(default)]
    pub mat2: Option<String>,
}

/// A member repeated along the line: a fence post, a lamp, a pier cap.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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

/// A strut repeated along the line, running across it on a slant: a knee
/// brace under a platform from the deck's edge in to the trunk. Its ends are
/// [across, up] in metres, across measured left of the line as for a rail
/// (`side: "right"` and `"both"` mirror it), up from the deck surface.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
pub struct BraceDef {
    #[serde(default = "both")]
    pub side: String,
    pub every: f32,
    #[serde(default)]
    pub phase: f32,
    pub from: [f32; 2],
    pub to: [f32; 2],
    #[serde(default = "quarter")]
    pub thick: f32,
    pub mat: String,
}

/// What holds a deck up where it stands clear of the ground.
#[derive(Deserialize, Clone, Debug, schemars::JsonSchema)]
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
    pub rubble: f32,
    pub mat2: u8,
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

#[derive(Clone, Debug)]
pub struct Brace {
    pub side: Side,
    pub every: f32,
    pub phase: f32,
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub thick: f32,
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
    pub braces: Vec<Brace>,
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
                rubble: r.rubble,
                mat2: r.mat2.as_deref().map_or(0, &mut mat),
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
        let braces: Vec<Brace> = def
            .braces
            .iter()
            .map(|b| Brace {
                side: Side::read(&b.side),
                every: b.every.max(0.5),
                phase: b.phase,
                from: b.from,
                to: b.to,
                thick: b.thick.max(0.25),
                mat: mat(&b.mat),
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
            // A thin rail is drawn a voxel tall about its middle (`voxel`).
            rise = rise.max(r.to).max((r.from + r.to) * 0.5 + 0.125);
        }
        for p in &posts {
            reach = reach.max(half + p.out + p.across);
            rise = rise.max(p.base + p.up);
        }
        if let Some(p) = &def.piers {
            reach = reach.max(p.radius);
        }
        let mut drop = deck_thick;
        for b in &braces {
            reach = reach.max(b.from[0].abs().max(b.to[0].abs()) + b.thick);
            rise = rise.max(b.from[1].max(b.to[1]) + b.thick);
            drop = drop.max(-(b.from[1].min(b.to[1])) + b.thick);
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
            braces,
            piers,
            sag: def.sag,
            span: None,
            clear: def.clear,
            lift: def.lift,
            max_span: def.max_span,
            reach: reach + 0.3,
            rise,
            drop,
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
            // A rail less than a voxel tall takes the one voxel row nearest
            // its middle: tested as it stands, a 0.2 m band misses every
            // voxel centre on level ground at some heights and the whole
            // rail is gone.
            let (from, to) = if r.to - r.from < VOXEL {
                let mid = (r.from + r.to) * 0.5;
                (mid - VOXEL * 0.5, mid + VOXEL * 0.5)
            } else {
                (r.from, r.to)
            };
            if !r.side.has(across) || up < from || up >= to {
                continue;
            }
            let (lo, hi) = band(self.half, r.out, r.thick, r.side);
            if r.rubble > 0.0 {
                if let Some(m) = rubble(r, a - lo, hi - lo, up, s) {
                    return Some(m);
                }
                continue;
            }
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
        for b in &self.braces {
            if (s - b.phase).rem_euclid(b.every) >= b.thick {
                continue;
            }
            // Mirrored to the right for a right-hand or two-sided strut.
            let sides: &[f32] = match b.side {
                Side::Left | Side::Centre => &[1.0],
                Side::Right => &[-1.0],
                Side::Both => &[1.0, -1.0],
            };
            for &m in sides {
                let (ax, ay) = (b.from[0] * m, b.from[1]);
                let (bx, by) = (b.to[0] * m, b.to[1]);
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((across - ax) * dx + (up - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
                let (qx, qy) = (ax + dx * t - across, ay + dy * t - up);
                if qx * qx + qy * qy < b.thick * b.thick * 0.25 {
                    return Some(b.mat);
                }
            }
        }
        None
    }
}

/// A heaped stone rail at `x` m into its band (of width `w`), `up` m above
/// the ground and `s` m along: which stone is there, if any. Stones are laid
/// in courses about 0.3 m high, each 0.4 to 0.9 m long and offset from the
/// course below; each has its own width and top, and the whole narrows
/// toward the top, so it reads as a pile and not a bar.
fn rubble(r: &Rail, x: f32, w: f32, up: f32, s: f32) -> Option<u8> {
    use crate::world::noise::{hash32, unit};
    let tall = r.to - r.from;
    let rel = ((up - r.from) / tall.max(0.1)).clamp(0.0, 1.0);
    let course = ((up - r.from) / 0.3).floor() as i32;
    let shift = unit(hash32(course as u32 ^ 0x51ab)) * 0.9;
    let len = 0.5;
    let k = ((s + shift) / len).floor() as i32;
    let h = hash32((k as u32).wrapping_mul(0x9e37_79b9) ^ (course as u32).wrapping_mul(0x85eb_ca6b));
    // The stone's top: the courses stop short in places along the wall.
    let along_top = r.to - r.rubble * tall * unit(hash32((((s + 0.3) / 1.4).floor() as i32) as u32 ^ 0x7f4a));
    if up >= along_top {
        return None;
    }
    // Battered: full width at the foot, about two thirds at the top, and
    // each stone bulging or sitting back a little.
    // A centred rail's band is half its width (the other half is the
    // mirror across the line).
    let width = if r.side == Side::Centre { w * 2.0 } else { w };
    let half = width * 0.5 * (1.0 - 0.38 * rel) * (0.82 + 0.3 * unit(h >> 3));
    let off = (unit(h >> 9) - 0.5) * 0.12 * width;
    let centre = if r.side == Side::Centre { 0.0 } else { w * 0.5 };
    if (x - centre - off).abs() > half {
        return None;
    }
    Some(if r.mat2 != 0 && unit(h >> 17) < 0.4 { r.mat2 } else { r.mat })
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
