//! Cities imported from map data (`city.json`). Buildings are drawn
//! straight from their footprint polygons when a chunk is generated: walls,
//! window bays by floor, a door on the side facing the street, flat roofs with
//! parapets, hipped and gabled roofs with eaves, porches, chimneys. Nothing is
//! compiled to a voxel array per building, so a city of tens of thousands of
//! buildings costs only its polygons. Street surfaces are decided per voxel
//! column from the street centrelines, so kerbs come out straight; bridge decks
//! and piers are drawn the same way.

use super::fields::{Fields, BUCKET, CELL};
use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::seg_dist;
use crate::world::material::AIR;
use glam::{Vec2, Vec3};
use serde::Deserialize;

// ---- the file the importer writes -----------------------------------------------------

#[derive(Deserialize, Default)]
pub struct CityFile {
    #[serde(default)]
    pub buildings: Vec<BuildingDef>,
    #[serde(default)]
    pub streets: Vec<StreetDef>,
    #[serde(default)]
    pub decks: Vec<DeckDef>,
    /// x, z, radius, top.
    #[serde(default)]
    pub piers: Vec<[f32; 4]>,
    #[serde(default)]
    pub areas: Vec<AreaDef>,
    /// x, z, plant name.
    #[serde(default)]
    pub trees: Vec<(f32, f32, String)>,
    /// Street name blades and other lettered signs (world/places.rs).
    #[serde(default)]
    pub signs: Vec<SignDef>,
    /// Named stretches of the map: neighbourhoods, parks, streets.
    #[serde(default)]
    pub regions: Vec<RegionDef>,
}

/// One sign the importer places: the post goes in as a structure and the
/// board and its lettering are drawn as plates.
#[derive(Deserialize)]
pub struct SignDef {
    pub at: [f32; 2],
    /// The compass bearing the front of the board faces.
    #[serde(default)]
    pub bearing: f32,
    /// A style in `library/signs/`.
    pub style: String,
    pub text: Vec<String>,
}

/// A named area, as a ragged circle or an outline.
#[derive(Deserialize)]
pub struct RegionDef {
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub poly: Vec<[f32; 2]>,
    #[serde(default)]
    pub at: Option<[f32; 2]>,
    #[serde(default)]
    pub radius: f32,
    /// How far inside before the HUD names it; a tenth of the way across by
    /// default.
    #[serde(default)]
    pub inside: Option<f32>,
}

/// What a city is made of, and what to make of it instead: a library entry in
/// `library/cities/`, named by the `city` event (`"kit": "faerie"`, else
/// `modern`). The roles are the materials the drawing code asks for by job
/// rather than by name; `remap` catches the names the import file chooses for
/// itself, building by building, which no role covers. A theme is a kit with
/// `"like": "modern"`, a remap and a little ivy -- the same entry either way,
/// so there is nothing a world can set here that another world cannot reuse.
#[derive(Deserialize, Clone, Debug)]
#[allow(dead_code)] // like/version/introduced_by: see below
pub struct Kit {
    #[serde(default)]
    pub name: String,
    /// The kit this one starts from, and the library bookkeeping every entry
    /// carries (library/README.md). Nothing here reads them: `like` is
    /// resolved before the entry is parsed (src/inherit.rs), and the other two
    /// are for the catalogue. They are fields rather than skipped keys so a
    /// misspelt one is still reported.
    #[serde(default)]
    pub like: Option<String>,
    #[serde(default)]
    pub version: Option<u32>,
    #[serde(default)]
    pub introduced_by: Option<String>,
    /// Road surfaces and the paved areas between them.
    #[serde(default = "d_road")]
    pub road: String,
    #[serde(default = "d_pavement")]
    pub pavement: String,
    /// Window panes, and roof vents and other metalwork.
    #[serde(default = "d_glass")]
    pub glass: String,
    #[serde(default = "d_metal")]
    pub metal: String,
    /// A bridge: the deck underneath, its surface, and the railing.
    #[serde(default = "d_deck")]
    pub deck: String,
    #[serde(default = "d_road")]
    pub deck_top: String,
    #[serde(default = "d_rail")]
    pub rail: String,
    #[serde(default = "d_chimney")]
    pub chimney: String,
    #[serde(default = "d_porch")]
    pub porch_floor: String,
    /// Front doors, one per building by its own roll.
    #[serde(default = "d_doors")]
    pub doors: Vec<String>,
    /// What a building stands on where its floor is above the ground.
    #[serde(default = "d_plinths")]
    pub plinths: Vec<String>,
    /// Library names swapped for others: materials (walls, roofs, roads) and
    /// street tree species alike.
    #[serde(default)]
    pub remap: HashMap<String, String>,
    /// How far ivy climbs the walls, 0 none to 1 most walls covered.
    #[serde(default)]
    pub ivy: f32,
    /// The ivy material (name it with LEAVES so it grows leaf sprites).
    #[serde(default)]
    pub ivy_mat: Option<String>,
    /// Extra street trees: each tree in the import gets this many more beside it
    /// along its row (0.5 = every other one doubles).
    #[serde(default)]
    pub tree_boost: f32,
    /// What a generated town (`town` event, `world/town/`) is made of.
    #[serde(default)]
    pub town: TownKit,
}

/// The materials of a generated town, by district style (`grid`, `radial`,
/// `maze`, `suburb`, `village`) where a style changes them. Each list is
/// drawn from at random, so a material named twice comes up twice as often.
#[derive(Deserialize, Clone, Debug, Default)]
pub struct TownKit {
    #[serde(default)]
    pub walls: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub roofs: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub trims: Vec<String>,
    /// Street surfaces: by rank (`main`, `ring`, `street`, `lane`, `close`,
    /// `alley`, `country`), or `<style>_<rank>` for one style only
    /// (`maze_lane`).
    #[serde(default)]
    pub streets: HashMap<String, String>,
    #[serde(default)]
    pub plaza: Option<String>,
    /// A flat roof's covering.
    #[serde(default)]
    pub flat_roof: Option<String>,
    /// The church's walls.
    #[serde(default)]
    pub church: Option<String>,
    /// Street trees (the first) and park trees (any).
    #[serde(default)]
    pub trees: Vec<String>,
    /// Route kits (`library/kits/`) for the field walls, the rail fences and
    /// the town wall.
    #[serde(default)]
    pub field_wall: Option<String>,
    #[serde(default)]
    pub rail: Option<String>,
    #[serde(default)]
    pub town_wall: Option<String>,
    /// Back-yard and garden fences.
    #[serde(default)]
    pub garden: Option<String>,
    /// Hedges between fields.
    #[serde(default)]
    pub hedge: Option<String>,
    /// Library structures for the sheds and shacks in field corners
    /// (`field_shed` and `field_shack` where the kit names none).
    #[serde(default)]
    pub shacks: Vec<String>,
}

/// What a town or a farm is made of where its kit names nothing. A world
/// reaches the ones its kit needs (`TownKit::defaults_used`).
pub mod town_default {
    pub const PLAZA: &str = "stone_tiles";
    pub const WALL: &str = "stucco_tan";
    pub const ROOF: &str = "slate";
    pub const TRIM: &str = "trim_white";
    pub const TREE: &str = "street_maple";
    pub const STREET: &str = "road_asphalt";
    pub const DRIVE: &str = "pavement";
    /// Route kits.
    pub const FIELD_WALL: &str = "fieldstone_wall";
    pub const FENCE: &str = "timber_fence";
    pub const TOWN_WALL: &str = "dry_stone_wall";
    pub const HEDGE: &str = "hedgerow";
    /// Sheds in field corners.
    pub const SHACKS: [&str; 2] = ["field_shed", "field_shack"];
    /// The region a town paints over its parks, for a biome to take.
    pub const PARK_REGION: &str = "city_park";
}

/// The district styles a kit's `walls` and `roofs` are keyed by.
pub const STYLE_KEYS: [&str; 6] = ["grid", "radial", "maze", "suburb", "village", "ribbon"];

impl TownKit {
    pub fn plaza_mat(&self) -> &str {
        self.plaza.as_deref().unwrap_or(town_default::PLAZA)
    }
    /// The wall materials for a district style: the first of `lists` (the
    /// event's own, then usually the kit's `walls`) that has any for it.
    pub fn walls_for(lists: &[&HashMap<String, Vec<String>>], style: &str) -> Vec<String> {
        Self::list_for(lists, style, town_default::WALL)
    }
    /// The roof materials for a district style, as `walls_for`.
    pub fn roofs_for(lists: &[&HashMap<String, Vec<String>>], style: &str) -> Vec<String> {
        Self::list_for(lists, style, town_default::ROOF)
    }
    fn list_for(lists: &[&HashMap<String, Vec<String>>], style: &str, or: &str) -> Vec<String> {
        lists.iter().find_map(|l| l.get(style).filter(|l| !l.is_empty())).cloned().unwrap_or_else(|| vec![or.to_string()])
    }
    pub fn trim_list(&self) -> Vec<String> {
        if self.trims.is_empty() {
            vec![town_default::TRIM.to_string()]
        } else {
            self.trims.clone()
        }
    }
    /// Street trees (the first), park trees (any) and hedgerow trees (the last).
    pub fn tree_list(&self) -> Vec<String> {
        if self.trees.is_empty() {
            vec![town_default::TREE.to_string()]
        } else {
            self.trees.clone()
        }
    }
    pub fn shack_list(&self) -> Vec<String> {
        if self.shacks.is_empty() {
            town_default::SHACKS.iter().map(|s| s.to_string()).collect()
        } else {
            self.shacks.clone()
        }
    }
    pub fn field_wall_kit(&self) -> &str {
        self.field_wall.as_deref().unwrap_or(town_default::FIELD_WALL)
    }
    pub fn rail_kit(&self) -> &str {
        self.rail.as_deref().unwrap_or(town_default::FENCE)
    }
    pub fn garden_kit(&self) -> &str {
        self.garden.as_deref().unwrap_or(town_default::FENCE)
    }
    pub fn town_wall_kit(&self) -> &str {
        self.town_wall.as_deref().unwrap_or(town_default::TOWN_WALL)
    }
    pub fn hedge_kit(&self) -> &str {
        self.hedge.as_deref().unwrap_or(town_default::HEDGE)
    }
    /// The route kits this kit names for its walls, fences and hedges.
    pub fn route_kits(&self) -> impl Iterator<Item = &String> {
        [&self.field_wall, &self.rail, &self.town_wall, &self.garden, &self.hedge].into_iter().flatten()
    }

    /// The `town_default` entries a town or farm of this kit falls back on:
    /// one for each role the kit leaves out. Walls and roofs count as left
    /// out unless every district style has its own.
    pub fn defaults_used(&self) -> Vec<&'static str> {
        use town_default::*;
        let partial = |m: &HashMap<String, Vec<String>>| STYLE_KEYS.iter().any(|k| m.get(*k).is_none_or(|l| l.is_empty()));
        let mut out = Vec::new();
        let mut want = |left_out: bool, name: &'static str| {
            if left_out {
                out.push(name);
            }
        };
        want(self.plaza.is_none(), PLAZA);
        want(partial(&self.walls), WALL);
        want(partial(&self.roofs), ROOF);
        want(self.trims.is_empty(), TRIM);
        want(self.trees.is_empty(), TREE);
        want(self.field_wall.is_none(), FIELD_WALL);
        want(self.rail.is_none() || self.garden.is_none(), FENCE);
        want(self.town_wall.is_none(), TOWN_WALL);
        want(self.hedge.is_none(), HEDGE);
        out
    }
}

fn d_road() -> String {
    "road_asphalt".into()
}
fn d_pavement() -> String {
    "pavement".into()
}
fn d_glass() -> String {
    "window_glass".into()
}
fn d_metal() -> String {
    "roof_vent".into()
}
fn d_deck() -> String {
    "concrete".into()
}
fn d_rail() -> String {
    "bridge_steel".into()
}
fn d_chimney() -> String {
    "brick_brown".into()
}
fn d_porch() -> String {
    "door_wood".into()
}
fn d_doors() -> Vec<String> {
    ["door_wood", "door_red", "door_blue", "door_green", "door_black", "door_yellow", "door_black"]
        .map(String::from)
        .to_vec()
}
fn d_plinths() -> Vec<String> {
    ["concrete", "brick_brown", "fieldstone"].map(String::from).to_vec()
}

impl Default for Kit {
    fn default() -> Self {
        serde_json::from_str("{}").expect("every field of a city kit has a default")
    }
}

impl Kit {
    /// Every library name this kit reaches, for validation and for a world's
    /// material ids (`worldload::reach`).
    pub fn names(&self) -> impl Iterator<Item = &String> {
        [
            &self.road,
            &self.pavement,
            &self.glass,
            &self.metal,
            &self.deck,
            &self.deck_top,
            &self.rail,
            &self.chimney,
            &self.porch_floor,
        ]
        .into_iter()
        .chain(self.doors.iter())
        .chain(self.plinths.iter())
        .chain(self.ivy_mat.iter())
        .chain(self.remap.values())
        .chain(self.town.walls.values().flatten())
        .chain(self.town.roofs.values().flatten())
        .chain(self.town.trims.iter())
        .chain(self.town.streets.values())
        .chain(self.town.plaza.iter())
        .chain(self.town.flat_roof.iter())
        .chain(self.town.church.iter())
        .chain(self.town.trees.iter())
    }
}

#[derive(Deserialize)]
pub struct BuildingDef {
    pub id: u64,
    pub style: String,
    pub poly: Vec<[f32; 2]>,
    #[serde(default)]
    pub holes: Vec<Vec<[f32; 2]>>,
    pub height: f32,
    /// How far the roof rises above the wall top, metres. The importer works
    /// it out from the footprint so that the source `height` (which includes the
    /// roof) comes out right; 0 leaves the old guess in place.
    #[serde(default)]
    pub roof_rise: f32,
    pub floor: f32,
    pub roof: String,
    pub wall: String,
    pub roof_mat: String,
    pub trim: String,
    #[serde(default)]
    pub door: usize,
    /// The ridge's direction [x, z], over the longest wall's.
    #[serde(default)]
    pub ridge: Option<[f32; 2]>,
    /// How far the roof overhangs the walls, metres (0.5 when left out).
    #[serde(default)]
    pub eave: Option<f32>,
}

#[derive(Deserialize)]
pub struct StreetDef {
    pub pts: Vec<[f32; 2]>,
    pub width: f32,
    pub mat: String,
    /// Pavement width each side, past the planting strip.
    #[serde(default)]
    pub walk: f32,
    /// Planting strip between the kerb and the pavement, each side.
    #[serde(default)]
    pub strip: f32,
}

#[derive(Deserialize)]
pub struct DeckDef {
    pub pts: Vec<[f32; 3]>,
    pub width: f32,
}

#[derive(Deserialize)]
pub struct AreaDef {
    pub kind: String,
    pub poly: Vec<[f32; 2]>,
}

// ---- the built city -------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Style {
    House,
    Shop,
    Block,
    Industrial,
    Church,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Roof {
    Flat,
    Hip,
    Gable,
}

pub struct Building {
    /// Ring 0 is the outline, counter-clockwise; the rest are courtyards.
    pub rings: Vec<Vec<Vec2>>,
    pub lo: Vec2,
    pub hi: Vec2,
    /// How far outside the outline things are drawn (eaves, porch).
    pub reach: f32,
    pub base: f32,
    /// Height of the raised ground floor above the pad (houses).
    pub plinth: f32,
    pub wall_top: f32,
    pub floor: f32,
    pub style: Style,
    pub roof: Roof,
    /// Roof rise per metre in from the eaves, and its cap.
    pub pitch: f32,
    pub max_rise: f32,
    /// Ridge direction (gables) and the ridge's offset and half-span across it.
    pub axis: Vec2,
    pub ridge: f32,
    pub half_w: f32,
    pub eave: f32,
    pub wall: u8,
    pub trim: u8,
    pub roof_mat: u8,
    pub door_mat: u8,
    pub plinth_mat: u8,
    pub door_edge: usize,
    /// Door centre along its edge, metres from the edge's start.
    pub door_t: f32,
    /// Porch depth and half-length, or 0.
    pub porch: (f32, f32),
    /// Chimney centre and top, or top 0 for none.
    pub chimney: (Vec2, f32),
    /// The door has a glazed upper panel.
    pub door_glass: bool,
    pub win_w: f32,
    pub bay: f32,
    pub seed: u32,
    /// Stands on water (a houseboat): a hull under the floor.
    pub afloat: bool,
}

pub struct Deck {
    pub a: Vec3,
    pub b: Vec3,
    pub half_w: f32,
}

pub struct Pier {
    pub at: Vec2,
    pub r: f32,
    pub top: f32,
}

pub struct Street {
    pub a: Vec2,
    pub b: Vec2,
    pub half: f32,
    pub strip: f32,
    pub walk: f32,
    pub mat: u8,
    /// The street stops square at `a` (or `b`): an end of its line that
    /// meets no other street, where a round end would wrap the pavement
    /// round it in a loop.
    pub square_a: bool,
    pub square_b: bool,
}

#[derive(Default)]
pub struct City {
    pub buildings: Vec<Building>,
    pub decks: Vec<Deck>,
    pub piers: Vec<Pier>,
    pub streets: Vec<Street>,
    /// Trees to plant once the world exists: position, plant name.
    pub trees: Vec<(Vec2, String)>,
    pub glass: u8,
    pub metal: u8,
    pub deck: u8,
    pub deck_top: u8,
    pub rail: u8,
    pub pavement: u8,
    pub chimney: u8,
    pub porch_floor: u8,
    pub ivy: f32,
    pub ivy_mat: u8,
    grid: HashMap<(i32, i32), Vec<u32>>,
    street_grid: HashMap<(i32, i32), Vec<u32>>,
}

const KIND_SHIFT: u32 = 30;
const DECK: u32 = 1 << KIND_SHIFT;
const PIER: u32 = 2 << KIND_SHIFT;
const ID_MASK: u32 = (1 << KIND_SHIFT) - 1;

fn bucket(v: f32) -> i32 {
    (v / BUCKET).floor() as i32
}

impl City {
    /// Is the point inside a building or within `margin` m of its walls?
    pub fn near_building(&self, p: Vec2, margin: f32) -> bool {
        let Some(list) = self.grid.get(&(bucket(p.x), bucket(p.y))) else {
            return false;
        };
        list.iter().filter(|&&id| id & !ID_MASK == 0).any(|&id| {
            let b = &self.buildings[(id & ID_MASK) as usize];
            p.x >= b.lo.x - margin && p.x <= b.hi.x + margin && p.y >= b.lo.y - margin && p.y <= b.hi.y + margin && (inside(&b.rings, p) || nearest_edge(&b.rings, p).0 < margin)
        })
    }

    /// Is the point under (or within `margin` m beside) a bridge deck?
    pub fn under_deck(&self, p: Vec2, margin: f32) -> bool {
        self.decks.iter().any(|d| seg_dist(Vec2::new(d.a.x, d.a.z), Vec2::new(d.b.x, d.b.z), p).0 < d.half_w + margin)
    }

    pub fn is_empty(&self) -> bool {
        self.buildings.is_empty() && self.decks.is_empty() && self.piers.is_empty() && self.streets.is_empty()
    }

    /// Take in another city's buildings, streets, decks, piers and trees
    /// (a second `town` or `city` event). The first city's materials for
    /// glass, decks and the rest stand for both.
    pub fn absorb(&mut self, other: City) {
        if self.buildings.is_empty() && self.streets.is_empty() && self.decks.is_empty() {
            *self = other;
            return;
        }
        self.buildings.extend(other.buildings);
        self.decks.extend(other.decks);
        self.piers.extend(other.piers);
        self.streets.extend(other.streets);
        self.trees.extend(other.trees);
    }

    pub fn index(&mut self) {
        let mut grid: HashMap<(i32, i32), Vec<u32>> = HashMap::default();
        let add = |g: &mut HashMap<(i32, i32), Vec<u32>>, lo: Vec2, hi: Vec2, id: u32| {
            for bz in bucket(lo.y)..=bucket(hi.y) {
                for bx in bucket(lo.x)..=bucket(hi.x) {
                    g.entry((bx, bz)).or_default().push(id);
                }
            }
        };
        for (k, bl) in self.buildings.iter().enumerate() {
            add(&mut grid, bl.lo - Vec2::splat(bl.reach), bl.hi + Vec2::splat(bl.reach), k as u32);
        }
        for (k, d) in self.decks.iter().enumerate() {
            let (lo, hi) = (Vec2::new(d.a.x.min(d.b.x), d.a.z.min(d.b.z)), Vec2::new(d.a.x.max(d.b.x), d.a.z.max(d.b.z)));
            add(&mut grid, lo - Vec2::splat(d.half_w), hi + Vec2::splat(d.half_w), DECK | k as u32);
        }
        for (k, p) in self.piers.iter().enumerate() {
            add(&mut grid, p.at - Vec2::splat(p.r), p.at + Vec2::splat(p.r), PIER | k as u32);
        }
        let mut sgrid = HashMap::default();
        for (k, s) in self.streets.iter().enumerate() {
            let r = Vec2::splat(s.half + s.strip + s.walk);
            add(&mut sgrid, s.a.min(s.b) - r, s.a.max(s.b) + r, k as u32);
        }
        self.grid = grid;
        self.street_grid = sgrid;
    }

    /// Items whose bounds may touch the box, metres.
    pub fn near(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<u32> {
        let mut out = Vec::new();
        if self.grid.is_empty() {
            return out;
        }
        for bz in bucket(z0)..=bucket(z1) {
            for bx in bucket(x0)..=bucket(x1) {
                if let Some(list) = self.grid.get(&(bx, bz)) {
                    out.extend(list);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The street surface at a point: a carriageway or path's own material,
    /// the pavement, or 0 (the planting strip and everything off the street).
    /// Decided per voxel column so kerbs are straight.
    pub fn street_at(&self, x: f32, z: f32) -> u8 {
        let Some(list) = self.street_grid.get(&(bucket(x), bucket(z))) else {
            return 0;
        };
        let p = Vec2::new(x, z);
        let (mut road, mut road_half, mut walk) = (0u8, 0.0f32, 0u8);
        for &k in list {
            let s = &self.streets[k as usize];
            let ab = s.b - s.a;
            let t = (p - s.a).dot(ab) / ab.length_squared().max(1e-6);
            if (t < 0.0 && s.square_a) || (t > 1.0 && s.square_b) {
                continue;
            }
            let (d, _) = seg_dist(s.a, s.b, p);
            if d <= s.half {
                if s.half > road_half {
                    road = s.mat;
                    road_half = s.half;
                }
            } else if s.walk > 0.0 && d >= s.half + s.strip && d <= s.half + s.strip + s.walk {
                walk = self.pavement;
            }
        }
        if road != 0 {
            road
        } else {
            walk
        }
    }

    /// The lowest and highest heights the items reach, metres.
    pub fn y_range(&self, items: &[u32]) -> (f32, f32) {
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &id in items {
            let k = (id & ID_MASK) as usize;
            match id & !ID_MASK {
                0 => {
                    let b = &self.buildings[k];
                    lo = lo.min(b.base - 1.0);
                    hi = hi.max(b.wall_top + b.max_rise + 1.5).max(b.chimney.1 + 0.5);
                }
                DECK => {
                    let d = &self.decks[k];
                    lo = lo.min(d.a.y.min(d.b.y) - 1.5);
                    hi = hi.max(d.a.y.max(d.b.y) + 1.5);
                }
                _ => {
                    let p = &self.piers[k];
                    lo = lo.min(0.0);
                    hi = hi.max(p.top + 0.5);
                }
            }
        }
        (lo, hi)
    }
}

// ---- geometry -------------------------------------------------------------------------

fn inside(rings: &[Vec<Vec2>], p: Vec2) -> bool {
    let mut odd = false;
    for r in rings {
        let n = r.len();
        let mut j = n - 1;
        for i in 0..n {
            let (a, b) = (r[i], r[j]);
            if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
                odd = !odd;
            }
            j = i;
        }
    }
    odd
}

/// Distance to the nearest wall, which ring and edge it is, how far along the
/// edge the nearest point lies and the edge's length.
fn nearest_edge(rings: &[Vec<Vec2>], p: Vec2) -> (f32, usize, usize, f32, f32) {
    let mut best = (f32::MAX, 0, 0, 0.0, 0.0);
    for (ri, r) in rings.iter().enumerate() {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            let ab = b - a;
            let len = ab.length().max(1e-4);
            let t = ((p - a).dot(ab) / (len * len)).clamp(0.0, 1.0);
            let d = p.distance(a + ab * t);
            if d < best.0 {
                best = (d, ri, i, t * len, len);
            }
        }
    }
    best
}

/// Mark the ends of each street line (`lines`, ranges of `streets`) that
/// meet no other line's carriageway: those stop square.
fn square_ends(streets: &mut [Street], lines: &[std::ops::Range<usize>]) {
    let meets = |streets: &[Street], own: &std::ops::Range<usize>, p: Vec2| {
        lines.iter().filter(|r| *r != own).flat_map(|r| r.clone()).any(|k| {
            let o = &streets[k];
            let reach = o.half + 0.5;
            p.x >= o.a.x.min(o.b.x) - reach
                && p.x <= o.a.x.max(o.b.x) + reach
                && p.y >= o.a.y.min(o.b.y) - reach
                && p.y <= o.a.y.max(o.b.y) + reach
                && seg_dist(o.a, o.b, p).0 <= reach
        })
    };
    for r in lines {
        if r.is_empty() {
            continue;
        }
        let (first, last) = (r.start, r.end - 1);
        // A line under a metre long is a disc (a turning circle), not a street.
        let len: f32 = streets[r.clone()].iter().map(|s| s.a.distance(s.b)).sum();
        // A closed ring has no ends.
        if len < 1.0 || streets[first].a.distance(streets[last].b) < 0.5 {
            continue;
        }
        let sa = !meets(streets, r, streets[first].a);
        let sb = !meets(streets, r, streets[last].b);
        streets[first].square_a = sa;
        streets[last].square_b = sb;
    }
}

fn hash(a: i32, b: i32, s: u32) -> f32 {
    super::noise::unit(super::noise::hash2(a, b, s))
}

// ---- building faces -------------------------------------------------------------------

impl Building {
    /// Height of the roof's eave line plane at `p`: the wall top at the walls,
    /// rising inward at the pitch (hip: from every wall; gable: from the two
    /// eave walls toward the ridge), falling outward under the eaves.
    fn roof_line(&self, p: Vec2, d_in: f32) -> f32 {
        match self.roof {
            Roof::Flat => self.wall_top,
            Roof::Hip => self.wall_top + (d_in * self.pitch).min(self.max_rise),
            Roof::Gable => {
                let n = Vec2::new(-self.axis.y, self.axis.x);
                let v = (p.dot(n) - self.ridge).abs();
                self.wall_top + ((self.half_w - v) * self.pitch).min(self.max_rise)
            }
        }
    }

    /// Is the edge `edge` of the outline a gable end (across the ridge)?
    fn gable_end(&self, edge: usize) -> bool {
        if self.roof != Roof::Gable {
            return false;
        }
        let r = &self.rings[0];
        let d = (r[(edge + 1) % r.len()] - r[edge]).normalize_or_zero();
        d.dot(self.axis).abs() < 0.35
    }
}

/// Window opening on a wall at height `yin` into floor `fl`, `t` metres along an
/// edge of length `len`. Returns 1 for a window, 2 for a shopfront, 0 for wall.
fn opening(b: &Building, fl: i32, yin: f32, t: f32, len: f32) -> u8 {
    if len < 2.0 || t < 0.6 || t > len - 0.6 {
        return 0;
    }
    let (bay, win, y0, y1, kind): (f32, f32, f32, f32, u8) = match (b.style, fl) {
        (Style::House, _) => (3.0, b.win_w, 0.85, 2.2, 1),
        (Style::Shop, 0) => (3.6, 3.0, 0.35, 3.1, 2),
        (Style::Shop, _) | (Style::Block, _) => (2.5, 1.4, 0.8, 2.5, 1),
        (Style::Industrial, _) => (6.0, 4.0, 3.9, 5.1, 1),
        (Style::Church, _) => (4.0, 1.3, 1.6, 5.6, 1),
    };
    if yin < y0 || yin >= y1 {
        return 0;
    }
    let span = len - 1.2;
    let bays = (span / (bay * b.bay)).round().max(1.0);
    let w = span / bays;
    let tt = (t - 0.6) % w;
    if (tt - w * 0.5).abs() < (win * 0.5).min(w * 0.4) {
        kind
    } else {
        0
    }
}

/// Floor index and height within it, `y` metres above the ground floor.
fn floor_of(b: &Building, y: f32) -> (i32, f32) {
    if b.style == Style::Shop {
        if y < 4.2 {
            (0, y)
        } else {
            (1 + ((y - 4.2) / b.floor) as i32, (y - 4.2) % b.floor)
        }
    } else {
        ((y / b.floor) as i32, y % b.floor)
    }
}

/// Ivy over a wall voxel: patches that climb from the ground and thin out with
/// height, a few reaching the eaves.
fn ivy_at(c: &City, b: &Building, p: Vec2, ym: f32) -> bool {
    if c.ivy <= 0.0 || c.ivy_mat == 0 {
        return false;
    }
    let y = ym - b.base;
    let n = super::noise::perlin3(p.x * 0.45, ym * 0.22, p.y * 0.45, b.seed) * 0.5 + 0.5;
    let reach = 3.0 + 14.0 * c.ivy * (super::noise::perlin2(p.x * 0.15, p.y * 0.15, b.seed ^ 7) * 0.5 + 0.5);
    let fine = super::noise::perlin3(p.x * 2.1, ym * 2.1, p.y * 2.1, b.seed ^ 3);
    (0.35 + 0.65 * n) * (1.0 - (y / reach).min(1.2)) + 0.12 * fine > 1.0 - c.ivy
}

/// The material for one voxel of a building column inside its outline, or None
/// to leave it. `d` is the distance in from the nearest wall, `vs` the voxel size.
#[allow(clippy::too_many_arguments)]
fn inside_voxel(c: &City, b: &Building, ym: f32, ground: f32, p: Vec2, d: f32, ring: usize, edge: usize, t: f32, len: f32, vs: f32) -> Option<u8> {
    let y = ym - b.base;
    if y < 0.0 {
        // A houseboat's hull, or footings down to ground the pad didn't reach.
        if b.afloat && y > -0.8 {
            return Some(b.trim);
        }
        return if ym > ground - 0.3 && y > -6.0 { Some(b.plinth_mat) } else { None };
    }
    let shell = d < vs;
    let behind = d < vs * 2.0;
    // The chimney stands through the roof.
    if b.chimney.1 > 0.0 && ym >= b.wall_top - 0.5 && ym < b.chimney.1 {
        let q = p - b.chimney.0;
        let n = Vec2::new(-b.axis.y, b.axis.x);
        if q.dot(b.axis).abs() < 0.45 && q.dot(n).abs() < 0.45 {
            return Some(if ym >= b.chimney.1 - 0.25 { c.metal } else { c.chimney });
        }
    }
    if ym >= b.wall_top {
        if b.roof == Roof::Flat {
            // A parapet at the edge, plant boxes and vents on top.
            let parapet = d < 0.3 + vs * 0.5;
            if parapet {
                return if ym < b.wall_top + 0.6 { Some(b.trim) } else { None };
            }
            if b.style != Style::House && d > 1.2 {
                let (cx, cz) = ((p.x / 3.0).floor() as i32, (p.y / 3.0).floor() as i32);
                let (fx, fz) = (p.x / 3.0 - cx as f32, p.y / 3.0 - cz as f32);
                if hash(cx, cz, b.seed) < 0.12 && (0.25..0.75).contains(&fx) && (0.3..0.7).contains(&fz) && ym < b.wall_top + 0.75 + 0.5 * hash(cz, cx, b.seed ^ 9) {
                    return Some(c.metal);
                }
            }
            return None;
        }
        let top = b.roof_line(p, d) + 0.3;
        if ym >= top {
            return None;
        }
        if ym >= top - 0.35 {
            return Some(b.roof_mat);
        }
        // Under the roof: the gable ends are wall, with a small attic window.
        if ring == 0 && shell && b.gable_end(edge) && (t - len * 0.5).abs() < 0.45 && ym - b.wall_top > 0.5 && ym - b.wall_top < 1.4 && ym < top - 0.8 {
            return Some(AIR);
        }
        if ring == 0 && behind && !shell && b.gable_end(edge) && (t - len * 0.5).abs() < 0.45 && ym - b.wall_top > 0.5 && ym - b.wall_top < 1.4 && ym < top - 0.8 {
            return Some(c.glass);
        }
        return Some(b.wall);
    }
    if b.roof == Roof::Flat && ym >= b.wall_top - vs {
        return Some(b.roof_mat);
    }
    if !behind {
        return Some(b.wall);
    }
    // The raised ground floor's plinth.
    if y < b.plinth {
        return Some(b.plinth_mat);
    }
    let yf = y - b.plinth;
    let (fl, yin) = floor_of(b, yf);
    // The door: recessed one voxel, a frame of trim round it, and on some
    // houses a glazed upper panel.
    if ring == 0 && edge == b.door_edge && len >= 2.0 {
        let along = (t - b.door_t).abs();
        if along < 0.5 && yf < 2.1 {
            if shell {
                return Some(AIR);
            }
            if b.style == Style::Shop {
                return Some(c.glass);
            }
            if b.door_glass && along < 0.25 && yf > 1.3 && yf < 1.85 {
                return Some(c.glass);
            }
            return Some(b.door_mat);
        }
        if shell && b.style != Style::Shop && along < 0.75 && yf < 2.35 {
            return Some(b.trim);
        }
    }
    let open = opening(b, fl, yin, t, len);
    if open > 0 && ym > ground + 0.25 {
        return Some(if shell { AIR } else { c.glass });
    }
    if shell {
        // Window frames on houses.
        if b.style == Style::House && ym > ground + 0.25 {
            let near = |dt: f32, dy: f32| opening(b, fl, yin + dy, t + dt, len) > 0;
            if near(0.22, 0.0) || near(-0.22, 0.0) || near(0.0, 0.22) || near(0.0, -0.22) {
                return Some(b.trim);
            }
        }
        // Trim: corners of houses, floor lines of blocks, the shop sign band.
        let corner = t < 0.25 || t > len - 0.25;
        let line = match b.style {
            Style::Block => fl > 0 && yin < 0.25,
            Style::Shop => fl == 0 && yin >= 3.4,
            _ => false,
        };
        if ivy_at(c, b, p, ym) {
            return Some(c.ivy_mat);
        }
        if corner && b.style == Style::House || line {
            return Some(b.trim);
        }
    }
    Some(b.wall)
}

/// The material for one voxel just outside the outline (eaves, porch, step),
/// `d` metres out from edge `edge`.
#[allow(clippy::too_many_arguments)]
fn outside_voxel(c: &City, b: &Building, ym: f32, p: Vec2, d: f32, ring: usize, edge: usize, t: f32) -> Option<u8> {
    let y = ym - b.base;
    if y < 0.0 {
        return None;
    }
    // Eaves: the roof runs on past the walls.
    if b.roof != Roof::Flat && d < b.eave && ring == 0 {
        let top = b.roof_line(p, -d) + 0.3;
        if ym < top && ym >= top - 0.35 {
            return Some(b.roof_mat);
        }
    }
    if ring != 0 || edge != b.door_edge || b.style != Style::House {
        return None;
    }
    let (depth, half) = b.porch;
    let along = (t - b.door_t).abs();
    let deck = b.plinth.max(0.25);
    if depth > 0.0 && d < depth && along < half {
        if y < deck {
            return Some(c.porch_floor);
        }
        let posts = d > depth - 0.3 && along > half - 0.3;
        if posts && y < deck + 2.45 {
            return Some(b.trim);
        }
        if y >= deck + 2.45 && y < deck + 2.7 {
            return Some(b.roof_mat);
        }
        return None;
    }
    // Steps up to the door (or the porch).
    let out = if depth > 0.0 { d - depth } else { d };
    if along < 0.6 && out >= 0.0 && b.plinth > 0.2 {
        let step = (out / 0.3).floor();
        let h = deck - 0.25 * (step + 1.0);
        if y < h {
            return Some(b.plinth_mat);
        }
    }
    None
}

/// Draw the city items in `list` into a padded chunk whose first voxel's corner
/// is at `o`. `ground(px, pz)` is the terrain height of a padded column.
#[allow(clippy::too_many_arguments)]
pub fn draw(c: &City, list: &[u32], v: &mut [u8], o: Vec3, vsize: f32, pad: usize, pady: usize, idx: impl Fn(usize, usize, usize) -> usize, ground: impl Fn(usize, usize) -> f32) -> bool {
    let mut any = false;
    // Only items whose height range meets this chunk's; buildings also get the
    // padded rows that range covers (one cell of slack each way).
    let (y0, y1) = (o.y, o.y + pady as f32 * vsize);
    let rows = |lo: f32, hi: f32| {
        let a = ((lo - o.y) / vsize - 1.0).floor().max(0.0) as usize;
        let b = ((hi - o.y) / vsize + 1.0).ceil().clamp(0.0, pady as f32) as usize;
        a..b.max(a)
    };
    let list: Vec<(u32, std::ops::Range<usize>)> = list
        .iter()
        .filter_map(|&id| {
            let k = (id & ID_MASK) as usize;
            let (lo, hi) = match id & !ID_MASK {
                0 => {
                    let b = &c.buildings[k];
                    (b.base - 6.5, (b.wall_top + b.max_rise + 2.0).max(b.chimney.1 + 0.5))
                }
                DECK => {
                    let dk = &c.decks[k];
                    (dk.a.y.min(dk.b.y) - 1.5, dk.a.y.max(dk.b.y) + 1.5)
                }
                _ => (f32::MIN, c.piers[k].top + 0.5),
            };
            (lo < y1 && hi > y0).then(|| (id, rows(lo, hi)))
        })
        .collect();
    if list.is_empty() {
        return false;
    }
    for pz in 0..pad {
        let zm = o.z + (pz as f32 + 0.5) * vsize;
        for px in 0..pad {
            let xm = o.x + (px as f32 + 0.5) * vsize;
            let p = Vec2::new(xm, zm);
            let g = ground(px, pz);
            for (id, ys) in &list {
                let id = *id;
                let k = (id & ID_MASK) as usize;
                match id & !ID_MASK {
                    0 => {
                        let b = &c.buildings[k];
                        let r = b.reach;
                        if p.x < b.lo.x - r || p.x > b.hi.x + r || p.y < b.lo.y - r || p.y > b.hi.y + r {
                            continue;
                        }
                        let ins = inside(&b.rings, p);
                        let (d, ring, edge, t, len) = nearest_edge(&b.rings, p);
                        if !ins && d > r {
                            continue;
                        }
                        for py in ys.clone() {
                            let ym = o.y + (py as f32 + 0.5) * vsize;
                            let m = if ins {
                                inside_voxel(c, b, ym, g, p, d, ring, edge, t, len, vsize)
                            } else {
                                outside_voxel(c, b, ym, p, d, ring, edge, t)
                            };
                            if let Some(m) = m {
                                let i = idx(px, py, pz);
                                // Outside the walls nothing is carved, only added.
                                if ins || m != AIR {
                                    v[i] = m;
                                    any = true;
                                }
                            }
                        }
                    }
                    DECK => {
                        let dk = &c.decks[k];
                        let (a, b) = (Vec2::new(dk.a.x, dk.a.z), Vec2::new(dk.b.x, dk.b.z));
                        let (d, t) = seg_dist(a, b, p);
                        if d > dk.half_w {
                            continue;
                        }
                        let y = dk.a.y + (dk.b.y - dk.a.y) * t;
                        // Railings along the sides only: at a segment's ends the
                        // distance is round the end point and would draw a ring.
                        let edge = d > dk.half_w - 0.3 && t > 0.0 && t < 1.0;
                        for py in 0..pady {
                            let ym = o.y + (py as f32 + 0.5) * vsize;
                            let m = if ym < y && ym >= y - vsize {
                                c.deck_top
                            } else if ym < y && ym >= y - 1.0 {
                                c.deck
                            } else if edge && ym >= y && ym < y + 1.0 {
                                c.rail
                            } else {
                                continue;
                            };
                            v[idx(px, py, pz)] = m;
                            any = true;
                        }
                    }
                    _ => {
                        let pr = &c.piers[k];
                        if p.distance(pr.at) > pr.r {
                            continue;
                        }
                        for py in 0..pady {
                            let ym = o.y + (py as f32 + 0.5) * vsize;
                            if ym < pr.top && ym > g - 1.0 {
                                v[idx(px, py, pz)] = c.deck;
                                any = true;
                            }
                        }
                    }
                }
            }
        }
    }
    any
}

// ---- building the city from the file (the `city` chronicle event) ---------------------

fn ring_of(pts: &[[f32; 2]]) -> Vec<Vec2> {
    pts.iter().map(|p| Vec2::new(p[0], p[1])).collect()
}

/// Every cell whose centre is inside the rings.
fn cells_in(f: &Fields, rings: &[Vec<Vec2>], lo: Vec2, hi: Vec2) -> Vec<usize> {
    let n = f.n as i32;
    let mut out = Vec::new();
    for z in ((lo.y / CELL).floor() as i32).max(0)..=((hi.y / CELL).ceil() as i32).min(n - 1) {
        for x in ((lo.x / CELL).floor() as i32).max(0)..=((hi.x / CELL).ceil() as i32).min(n - 1) {
            let i = z as usize * f.n + x as usize;
            if inside(rings, f.centre(i)) {
                out.push(i);
            }
        }
    }
    out
}

fn bounds(rings: &[Vec<Vec2>]) -> (Vec2, Vec2) {
    let mut lo = Vec2::splat(f32::MAX);
    let mut hi = Vec2::splat(f32::MIN);
    for p in rings.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (lo, hi)
}

/// Paint every cell within `r` of the polyline.
fn paint_line(f: &mut Fields, pts: &[[f32; 2]], r: f32, mut g: impl FnMut(&mut Fields, usize)) {
    let n = f.n as i32;
    for w in pts.windows(2) {
        let (a, b) = (Vec2::new(w[0][0], w[0][1]), Vec2::new(w[1][0], w[1][1]));
        let (lo, hi) = (a.min(b) - Vec2::splat(r), a.max(b) + Vec2::splat(r));
        for z in ((lo.y / CELL).floor() as i32).max(0)..=((hi.y / CELL).ceil() as i32).min(n - 1) {
            for x in ((lo.x / CELL).floor() as i32).max(0)..=((hi.x / CELL).ceil() as i32).min(n - 1) {
                let i = z as usize * f.n + x as usize;
                if seg_dist(a, b, f.centre(i)).0 <= r {
                    g(f, i);
                }
            }
        }
    }
}

/// Run the `city` event: streets and areas into the fields, buildings levelled
/// onto pads, decks, piers and street trees kept for later. `mat` resolves a
/// library material name (warning when it is missing).
pub fn build(f: &mut Fields, file: &CityFile, kit: &Kit, cause: u16, seed: u32, mut base_mat: impl FnMut(&str) -> u8) -> (City, String) {
    let sea = f.sea;
    let mut mat = |name: &str| base_mat(kit.remap.get(name).map_or(name, |s| s.as_str()));
    // Areas: parks and woods become regions for biomes, paved areas get a surface.
    let paved = mat(&kit.road);
    let mut regions = 0;
    for a in &file.areas {
        let rings = vec![ring_of(&a.poly)];
        let (lo, hi) = bounds(&rings);
        let cells = cells_in(f, &rings, lo, hi);
        if a.kind == "paved" {
            for i in cells {
                if f.h[i] > sea {
                    f.surface[i] = paved;
                    f.occupied[i] = true;
                }
            }
            continue;
        }
        let r = f.region_id(&format!("city_{}", a.kind));
        for i in cells {
            f.region[i] = r;
            f.cause[i] = cause;
        }
        regions += 1;
    }
    let mut city = City {
        glass: mat(&kit.glass),
        metal: mat(&kit.metal),
        deck: mat(&kit.deck),
        deck_top: mat(&kit.deck_top),
        pavement: mat(&kit.pavement),
        rail: mat(&kit.rail),
        chimney: mat(&kit.chimney),
        porch_floor: mat(&kit.porch_floor),
        ivy: kit.ivy,
        ivy_mat: kit.ivy_mat.as_deref().map_or(0, &mut mat),
        ..City::default()
    };
    // Streets: kept as segments (the generator draws the surface per voxel
    // column); the fields only learn that nothing grows there.
    let mut lines: Vec<std::ops::Range<usize>> = Vec::with_capacity(file.streets.len());
    for s in &file.streets {
        let m = mat(&s.mat);
        let first = city.streets.len();
        for w in s.pts.windows(2) {
            city.streets.push(Street {
                a: Vec2::new(w[0][0], w[0][1]),
                b: Vec2::new(w[1][0], w[1][1]),
                half: s.width * 0.5,
                strip: s.strip,
                walk: s.walk,
                mat: m,
                square_a: false,
                square_b: false,
            });
        }
        lines.push(first..city.streets.len());
        paint_line(f, &s.pts, s.width * 0.5 + s.strip + s.walk, |f, i| {
            if f.h[i] > sea {
                f.occupied[i] = true;
                f.cause[i] = cause;
            }
        });
    }
    square_ends(&mut city.streets, &lines);
    // Buildings.
    let doors: Vec<u8> = kit.doors.iter().map(|d| mat(d)).collect();
    let plinths: Vec<u8> = kit.plinths.iter().map(|d| mat(d)).collect();
    let mut afloat = 0;
    let mut kinds = [0usize; 3];
    for bd in &file.buildings {
        if bd.poly.len() < 3 {
            continue;
        }
        let mut rings = vec![ring_of(&bd.poly)];
        rings.extend(bd.holes.iter().filter(|h| h.len() >= 3).map(|h| ring_of(h)));
        let (lo, hi) = bounds(&rings);
        let mut cells = cells_in(f, &rings, lo, hi);
        if cells.is_empty() {
            cells.push(f.idx((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5));
        }
        let wet = cells.iter().filter(|&&i| f.h[i] < sea).count();
        let on_water = wet * 2 > cells.len();
        let base = if on_water {
            afloat += 1;
            sea + 0.3
        } else {
            // On a slope the floor sits a little above the middle of the ground
            // under it: the downhill side stands on a foundation, the uphill
            // side is cut in a little, and nothing is buried to the eaves.
            let mut hs: Vec<f32> = cells.iter().filter(|&&i| f.h[i] >= sea).map(|&i| f.h[i]).collect();
            if hs.is_empty() {
                sea + 0.3
            } else {
                hs.sort_by(|a, b| a.partial_cmp(b).unwrap());
                hs[(hs.len() as f32 * 0.6) as usize % hs.len()]
            }
        };
        for &i in &cells {
            if !on_water && f.h[i] >= sea {
                f.h[i] = base;
            }
            f.occupied[i] = true;
            f.cause[i] = cause;
        }
        // Keep trees a little off the walls.
        let (blo, bhi) = (lo - Vec2::splat(2.0), hi + Vec2::splat(2.0));
        for i in cells_in(f, &[vec![blo, Vec2::new(bhi.x, blo.y), bhi, Vec2::new(blo.x, bhi.y)]], blo, bhi) {
            if nearest_edge(&rings, f.centre(i)).0 < 2.0 {
                f.occupied[i] = true;
            }
        }
        let style = match bd.style.as_str() {
            "house" => Style::House,
            "shop" => Style::Shop,
            "industrial" => Style::Industrial,
            "church" => Style::Church,
            _ => Style::Block,
        };
        let bseed = super::noise::hash2(bd.id as i32, (bd.id >> 32) as i32, seed);
        let u = |k: u32| super::noise::unit(super::noise::hash32(bseed ^ k.wrapping_mul(0x9e37_79b9)));
        // The ridge runs along the outline's longest wall.
        let outer = &rings[0];
        let mut axis = Vec2::X;
        let mut longest = 0.0;
        for i in 0..outer.len() {
            let e = outer[(i + 1) % outer.len()] - outer[i];
            if e.length() > longest {
                longest = e.length();
                axis = e / e.length();
            }
        }
        if let Some([x, z]) = bd.ridge {
            let v = Vec2::new(x, z);
            if v.length() > 1e-3 {
                axis = v.normalize();
            }
        }
        let nrm = Vec2::new(-axis.y, axis.x);
        let (mut v0, mut v1, mut a0, mut a1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for q in outer {
            v0 = v0.min(q.dot(nrm));
            v1 = v1.max(q.dot(nrm));
            a0 = a0.min(q.dot(axis));
            a1 = a1.max(q.dot(axis));
        }
        let half_w = (v1 - v0) * 0.5;
        let roof = match (bd.roof.as_str(), style) {
            ("gable", _) => Roof::Gable,
            ("hip", _) => Roof::Hip,
            _ => Roof::Flat,
        };
        let house = style == Style::House;
        let mut pitch = match (roof, style) {
            (Roof::Flat, _) => 0.0,
            (_, Style::Church) => 1.1,
            _ => 0.5 + 0.45 * u(1),
        };
        let mut max_rise = (pitch * half_w).min(if roof == Roof::Gable { 5.5 } else { 4.0 });
        // An import that worked the rise out itself: take it, and slope the
        // roof to match so the ridge lands where it was meant to.
        if bd.roof_rise > 0.0 && roof != Roof::Flat {
            max_rise = bd.roof_rise;
            pitch = (max_rise / half_w.max(0.5)).clamp(0.15, 1.6);
        }
        let plinth = if house && !on_water { 0.25 * (u(2) * 4.0).floor() } else { 0.0 };
        let wall_top = base + bd.height + plinth;
        // The door's edge, and where along it.
        let door_edge = bd.door.min(outer.len() - 1);
        let de = outer[(door_edge + 1) % outer.len()] - outer[door_edge];
        let dlen = de.length();
        let door_t = if house && dlen > 4.0 { dlen * (0.3 + 0.4 * u(3)) } else { dlen * 0.5 };
        let porch = if house && !on_water && dlen >= 4.0 && u(4) < 0.55 {
            let half = (1.2 + 1.2 * u(5)).min(door_t - 0.2).min(dlen - door_t - 0.2);
            if half > 0.9 {
                (1.8 + 0.6 * u(6), half)
            } else {
                (0.0, 0.0)
            }
        } else {
            (0.0, 0.0)
        };
        let chimney = if roof != Roof::Flat && house && u(7) < 0.55 {
            let c = axis * ((a0 + a1) * 0.5 + (a1 - a0) * (u(8) - 0.5) * 0.5) + nrm * ((v0 + v1) * 0.5 + half_w * 0.35 * (u(9) - 0.5));
            (c, wall_top + max_rise + 0.9)
        } else {
            (Vec2::ZERO, 0.0)
        };
        let eave: f32 = if roof == Roof::Flat { 0.0 } else { bd.eave.unwrap_or(0.5) };
        let reach = eave.max(if porch.0 > 0.0 { porch.0 + 0.1 } else { 0.0 }).max(if plinth > 0.2 { 1.5 } else { 0.0 });
        kinds[match roof {
            Roof::Flat => 0,
            Roof::Hip => 1,
            Roof::Gable => 2,
        }] += 1;
        city.buildings.push(Building {
            rings,
            lo,
            hi,
            reach,
            base,
            plinth,
            wall_top,
            floor: bd.floor,
            style,
            roof,
            pitch,
            max_rise,
            axis,
            ridge: (v0 + v1) * 0.5,
            half_w,
            eave,
            wall: mat(&bd.wall),
            trim: mat(&bd.trim),
            roof_mat: mat(&bd.roof_mat),
            door_mat: doors[(u(10) * doors.len() as f32) as usize % doors.len()],
            plinth_mat: plinths[(u(11) * 3.0) as usize % 3],
            door_edge,
            door_t,
            porch,
            chimney,
            door_glass: u(14) < 0.45,
            win_w: 0.9 + 0.5 * u(12),
            bay: 0.85 + 0.3 * u(13),
            seed: bseed,
            afloat: on_water,
        });
    }
    for d in &file.decks {
        for w in d.pts.windows(2) {
            city.decks.push(Deck {
                a: Vec3::from(w[0]),
                b: Vec3::from(w[1]),
                half_w: d.width * 0.5 + 0.6,
            });
        }
        // Nothing grows under a deck: crowns would come up through it.
        let flat: Vec<[f32; 2]> = d.pts.iter().map(|p| [p[0], p[2]]).collect();
        paint_line(f, &flat, d.width * 0.5 + 6.0, |f, i| f.occupied[i] = true);
    }
    for p in &file.piers {
        city.piers.push(Pier {
            at: Vec2::new(p[0], p[1]),
            r: p[2],
            top: p[3],
        });
    }
    let tree_name = |n: &String| kit.remap.get(n).cloned().unwrap_or_else(|| n.clone());
    city.trees = file.trees.iter().map(|(x, z, name)| (Vec2::new(*x, *z), tree_name(name))).collect();
    // A theme may plant more: another tree beside some of them, 3-5 m along.
    if kit.tree_boost > 0.0 {
        let base: Vec<(Vec2, String)> = city.trees.clone();
        for (k, (p, name)) in base.iter().enumerate() {
            let u = |j: u32| super::noise::unit(super::noise::hash32(seed ^ (k as u32).wrapping_mul(0x9e37_79b9) ^ j));
            if u(1) < kit.tree_boost {
                let a = u(2) * std::f32::consts::TAU;
                let r = 3.0 + 2.0 * u(3);
                city.trees.push((*p + Vec2::new(a.cos(), a.sin()) * r, name.clone()));
            }
        }
    }
    let msg = format!(
        "{} buildings ({} afloat; roofs {} flat, {} hip, {} gable), {} streets, {} trees, {} deck segments, {} piers, {} areas as regions",
        city.buildings.len(),
        afloat,
        kinds[0],
        kinds[1],
        kinds[2],
        file.streets.len(),
        city.trees.len(),
        city.decks.len(),
        city.piers.len(),
        regions
    );
    (city, msg)
}

#[cfg(test)]
mod kit_tests {
    use super::Kit;
    use crate::library::Library;

    /// A kit that says nothing is the city the importer meant: asphalt,
    /// concrete, tar. Every role has a default, so a two-line kit is legal.
    #[test]
    fn a_kit_that_says_nothing_is_the_plain_city() {
        let k = Kit::default();
        assert_eq!(k.road, "road_asphalt");
        assert_eq!(k.deck_top, "road_asphalt", "a bridge is surfaced like a road");
        assert_eq!(k.doors.len(), 7);
        assert_eq!(k.plinths.len(), 3);
        assert_eq!(k.ivy, 0.0);
        assert!(k.remap.is_empty());
    }

    /// The library's own kits: `modern` is the plain one, and `faerie` starts
    /// from it (`"like"`) and changes the ground underfoot without having to
    /// repeat the doors.
    #[test]
    fn the_library_kits_load_and_inherit() {
        let (lib, errs) = Library::load(std::path::Path::new("."));
        assert!(errs.is_empty(), "{errs:#?}");
        let modern = lib.city_kit("modern").expect("a modern city kit");
        assert_eq!(modern.road, "road_asphalt");
        let faerie = lib.city_kit("faerie").expect("a faerie city kit");
        assert_eq!(faerie.road, "fae_cobbles");
        assert_eq!(faerie.doors, modern.doors, "the doors came from modern");
        assert!(faerie.ivy > 0.0 && faerie.ivy_mat.is_some(), "the faerie city grows ivy");
        // Every name a kit reaches must be a material or a plant, or a world
        // using it would draw stone where it meant moss (checked for the
        // whole library by `Library::problems`).
        for n in faerie.names() {
            let known = lib.materials.iter().any(|m| &m.name == n) || lib.plants.iter().any(|p| &p.name == n);
            assert!(known, "faerie kit names {n}, which is neither a material nor a plant");
        }
    }
}
