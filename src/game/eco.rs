//! The animal debug view (notes/ANIMAL_SIM_PLAN.md, D1 to D5), turned on and
//! off with backquote. While it is on: a whole-world ecology map (P), an
//! inspector for one pinned animal (I) with its world lines, and time
//! controls for the animals (O freeze, `.` step one think, `,` ten times as
//! fast). Off, those keys do nothing, so none fires by accident in play.
//! A JSON snapshot serves headless soaks
//! (`--eco-dump`). With the map open, `[` and `]` step through its layers:
//! the animals, then every channel of the world survey (world/survey), then
//! the plant site fields (world/site.rs), each as a heatmap under them. Later layers of the plan (nodes, herd records) add to
//! `Layer` and to the snapshot.

use super::Game;
use crate::creatures::inspect::Inspect;
use crate::creatures::walk::{Critter, Mood};
use crate::creatures::{herd::Kin, spawn, Creature, Look};
use crate::render::hud::Hud;
use crate::world::site::{Site, SiteChan};
use crate::world::live::{Live, LiveChan};
use crate::world::survey::graph::{Class, Kind};
use crate::world::survey::{self, Chan, Scale, Survey};
use glam::Vec3;

/// What the map draws over the ground: the animals alone, or a channel of
/// the world survey under them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Layer {
    #[default]
    Creatures,
    Survey(Chan),
    /// A plant site field (world/site.rs).
    Site(SiteChan),
    /// Every plant with a site block as a dot, by how well its ground suits it.
    Fit,
    /// Every plant as a dot: grey where its biome grows it, bright where it
    /// came over a border from the neighbouring biome (placement v2's zones).
    Border,
    /// The realm overlays (world/realm_map.rs): each cell coloured by the
    /// strongest realm coming through it, brighter the stronger.
    Realm,
    /// A channel of the live grid (world/live.rs): what is happening now.
    Live(LiveChan),
    /// The survey's graph (world/survey/graph.rs): its zones, its anchors and
    /// the links for one kind of body.
    Graph(Class),
}

impl Layer {
    pub fn parse(s: &str) -> Option<Layer> {
        match s {
            "creatures" => Some(Layer::Creatures),
            "fit" => Some(Layer::Fit),
            "border" => Some(Layer::Border),
            "realm" => Some(Layer::Realm),
            _ => Chan::parse(s).map(Layer::Survey).or_else(|| SiteChan::parse(s).map(Layer::Site)).or_else(|| LiveChan::parse(s).map(Layer::Live))
                .or_else(|| Class::ALL.into_iter().find(|c| s.strip_prefix("links_") == Some(c.name())).map(Layer::Graph)),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Layer::Creatures => "creatures",
            Layer::Survey(c) => c.name(),
            Layer::Site(c) => c.name(),
            Layer::Fit => "fit",
            Layer::Border => "border",
            Layer::Realm => "realm",
            Layer::Live(c) => c.name(),
            Layer::Graph(Class::Person) => "links_person",
            Layer::Graph(Class::Large) => "links_large",
            Layer::Graph(Class::Vehicle) => "links_vehicle",
        }
    }

    /// Every layer, in the order `[` and `]` step through them.
    pub fn all() -> Vec<Layer> {
        std::iter::once(Layer::Creatures)
            .chain(Chan::ALL.iter().map(|&c| Layer::Survey(c)))
            .chain(Class::ALL.into_iter().map(Layer::Graph))
            .chain(SiteChan::ALL.iter().map(|&c| Layer::Site(c)))
            .chain([Layer::Fit, Layer::Border, Layer::Realm])
            .chain(LiveChan::ALL.iter().map(|&c| Layer::Live(c)))
            .collect()
    }

    /// Every layer's name, for an error message.
    pub fn names() -> String {
        Layer::all().iter().map(|l| l.name()).collect::<Vec<_>>().join(", ")
    }
}

/// The colour a survey channel's byte is drawn in, and how strongly over the
/// ground (0 leaves the ground as it is).
fn survey_colour(c: Chan, b: u8) -> ([f32; 3], f32) {
    let t = b as f32 / 255.0;
    match c.scale() {
        Scale::Flag if b >= 128 => ([60.0, 200.0, 90.0], 0.35),
        Scale::Flag => ([225.0, 45.0, 35.0], 0.7),
        Scale::Share if b == 0 => ([0.0; 3], 0.0),
        Scale::Share => (ramp(t), 0.85),
        // Near is bright; past the far end, nothing.
        Scale::Distance if b == 255 => ([0.0; 3], 0.0),
        Scale::Distance => (ramp(1.0 - (b as f32 / 64.0).min(1.0)), 0.8),
        Scale::Rise => {
            let reach = if c == Chan::Rise200 { 40.0 } else { 12.0 };
            let v = ((b as f32 - 128.0) / reach).clamp(-1.0, 1.0);
            let white = [235.0, 235.0, 235.0];
            let to = if v < 0.0 { [40.0, 90.0, 220.0] } else { [220.0, 50.0, 40.0] };
            let a = v.abs();
            (std::array::from_fn(|j| white[j] * (1.0 - a) + to[j] * a), 0.8)
        }
        Scale::Class => {
            const P: [[f32; 3]; 6] = [[0.0; 3], [150.0, 230.0, 255.0], [60.0, 160.0, 240.0], [20.0, 60.0, 200.0], [120.0, 220.0, 190.0], [20.0, 120.0, 110.0]];
            const PRINT: [[f32; 3]; 4] = [[0.0; 3], [120.0, 210.0, 80.0], [200.0, 150.0, 90.0], [250.0, 250.0, 250.0]];
            match (c, b) {
                (_, 0) => ([0.0; 3], 0.0),
                (Chan::Print, _) => (PRINT[(b as usize).min(3)], 0.8),
                _ => (P[(b as usize).min(5)], 0.85),
            }
        }
    }
}

/// Dark blue through teal and green to yellow, 0 to 1.
fn ramp(t: f32) -> [f32; 3] {
    const STOPS: [[f32; 3]; 5] = [[40.0, 30.0, 110.0], [40.0, 90.0, 170.0], [30.0, 160.0, 150.0], [110.0, 205.0, 80.0], [250.0, 230.0, 40.0]];
    let x = t.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    let f = x - i as f32;
    std::array::from_fn(|j| STOPS[i][j] * (1.0 - f) + STOPS[i + 1][j] * f)
}

/// The part of the world the map shows: its corner (x, z) and its edge,
/// metres.
pub type Win = (f32, f32, f32);

/// The most the map zooms in.
const MAX_ZOOM: f32 = 32.0;

/// A survey channel over the map's ground image (both `px` square).
fn survey_image(ground: &[u8], s: &Survey, c: Chan, win: Win, px: usize) -> Vec<u8> {
    let mut img = ground.to_vec();
    let step = win.2 / px as f32;
    let g = s.grid(c);
    for y in 0..px {
        for x in 0..px {
            let b = g[s.idx(win.0 + (x as f32 + 0.5) * step, win.1 + (y as f32 + 0.5) * step)];
            let (col, a) = survey_colour(c, b);
            let k = (y * px + x) * 4;
            for j in 0..3 {
                // The ground greyed under a layer, so the colours read.
                let under = img[k + j] as f32 * 0.55 + 20.0;
                img[k + j] = (under * (1.0 - a) + col[j] * a).clamp(0.0, 255.0) as u8;
            }
        }
    }
    img
}

/// The colour a herd's way is drawn in on the map, by what it is after.
fn need_colour(n: crate::creatures::roam::Need) -> [f32; 4] {
    use crate::creatures::roam::Need;
    match n {
        Need::Drink => [0.35, 0.8, 1.0, 0.95],
        Need::Feed => [0.5, 1.0, 0.4, 0.95],
        Need::Rest => [0.85, 0.7, 1.0, 0.95],
        Need::Away => [1.0, 0.45, 0.3, 0.95],
        Need::Hunt => [1.0, 0.15, 0.15, 0.95],
        Need::Home => [1.0, 0.85, 0.2, 0.95],
    }
}

/// The colour an anchor of each kind is drawn in on the map.
fn kind_colour(k: Kind) -> [f32; 4] {
    match k {
        Kind::Focal => [1.0, 1.0, 1.0, 1.0],
        Kind::Vantage => [1.0, 0.85, 0.2, 1.0],
        Kind::Pinch => [1.0, 0.25, 0.2, 1.0],
        Kind::Entry => [0.75, 0.45, 1.0, 1.0],
        Kind::Refuge => [0.95, 0.55, 0.2, 1.0],
        Kind::Nook => [0.6, 0.35, 0.2, 1.0],
        Kind::Drink => [0.3, 0.75, 1.0, 1.0],
        Kind::Feed => [0.4, 0.95, 0.35, 1.0],
    }
}

/// The survey graph over the map's ground image: each zone tinted its own
/// colour with its edge darker, and one class's links as lines (red where
/// a link goes through a pinch).
fn graph_image(ground: &[u8], s: &Survey, class: Class, win: Win, px: usize) -> Vec<u8> {
    let g = &s.graph;
    let mut img = ground.to_vec();
    let step = win.2 / px as f32;
    let tint = |z: u16| -> [f32; 3] {
        let h = (z as u32).wrapping_mul(2654435761);
        [70.0 + (h & 0x7f) as f32, 70.0 + (h >> 8 & 0x7f) as f32, 70.0 + (h >> 16 & 0x7f) as f32]
    };
    let zone_at = |x: usize, y: usize| g.zone_of.get(s.idx(win.0 + (x as f32 + 0.5) * step, win.1 + (y as f32 + 0.5) * step)).copied().unwrap_or(0);
    for y in 0..px {
        for x in 0..px {
            let z = zone_at(x, y);
            let k = (y * px + x) * 4;
            let edge = z != zone_at((x + 1).min(px - 1), y) || z != zone_at(x, (y + 1).min(px - 1));
            for j in 0..3 {
                let under = img[k + j] as f32 * 0.55 + 20.0;
                img[k + j] = if z == 0 {
                    (under * 0.6) as u8
                } else if edge {
                    (under * 0.35) as u8
                } else {
                    (under * 0.6 + tint(z)[j] * 0.4).clamp(0.0, 255.0) as u8
                };
            }
        }
    }
    let mut plot = |x: f32, y: f32, c: [u8; 3]| {
        let (xi, yi) = (x as i32, y as i32);
        if xi >= 0 && yi >= 0 && (xi as usize) < px && (yi as usize) < px {
            let k = (yi as usize * px + xi as usize) * 4;
            img[k..k + 3].copy_from_slice(&c);
        }
    };
    for l in g.links.iter().filter(|l| l.class == class) {
        let c = if l.pinch { [255, 70, 50] } else { [245, 245, 235] };
        for w in l.path.windows(2) {
            let (a, b) = ([(w[0][0] - win.0) / step, (w[0][1] - win.1) / step], [(w[1][0] - win.0) / step, (w[1][1] - win.1) / step]);
            let n = (b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                plot(a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, c);
            }
        }
    }
    img
}

/// A live channel over the map's ground image (both `px` square): nothing
/// where it is quiet, the ramp up to the channel's full level.
fn live_image(ground: &[u8], l: &Live, c: LiveChan, win: Win, px: usize) -> Vec<u8> {
    let mut img = ground.to_vec();
    let step = win.2 / px as f32;
    let g = l.grid(c);
    for y in 0..px {
        for x in 0..px {
            let v = g[l.idx(win.0 + (x as f32 + 0.5) * step, win.1 + (y as f32 + 0.5) * step)] / c.full();
            let k = (y * px + x) * 4;
            let (col, a) = if v < 0.01 { ([0.0; 3], 0.0) } else { (ramp(v), 0.35 + 0.5 * v.min(1.0)) };
            for j in 0..3 {
                let under = img[k + j] as f32 * 0.55 + 20.0;
                img[k + j] = (under * (1.0 - a) + col[j] * a).clamp(0.0, 255.0) as u8;
            }
        }
    }
    img
}

/// A site field over the map's ground image (both `px` square): the ramp,
/// read bilinearly so the 4 m cells don't show as blocks.
fn site_image(ground: &[u8], s: &Site, c: SiteChan, win: Win, px: usize) -> Vec<u8> {
    let mut img = ground.to_vec();
    let step = win.2 / px as f32;
    for y in 0..px {
        for x in 0..px {
            let (wx, wz) = (win.0 + (x as f32 + 0.5) * step, win.1 + (y as f32 + 0.5) * step);
            let col = ramp(s.value(c, wx, wz));
            let a = if s.water[s.idx(wx, wz)] { 0.25 } else { 0.85 };
            let k = (y * px + x) * 4;
            for j in 0..3 {
                let under = img[k + j] as f32 * 0.55 + 20.0;
                img[k + j] = (under * (1.0 - a) + col[j] * a).clamp(0.0, 255.0) as u8;
            }
        }
    }
    img
}

/// Red (0) through yellow to green (1): how well a plant's ground suits it.
fn fit_colour(f: f32) -> [f32; 3] {
    let f = f.clamp(0.0, 1.0);
    if f < 0.5 {
        [230.0, 50.0 + 360.0 * f, 40.0]
    } else {
        [230.0 - 380.0 * (f - 0.5), 230.0, 40.0 + 40.0 * (f - 0.5)]
    }
}

/// The plants over the map's ground image, greyed: each with a site block a
/// dot coloured by how well its ground suits it (every field but shade,
/// which is not kept after placement), the rest faint grey.
fn fit_image(ground: &[u8], w: &crate::world::fields::World, win: Win, px: usize) -> Vec<u8> {
    let mut img: Vec<u8> = ground
        .chunks(4)
        .flat_map(|c| {
            let g = ((c[0] as u32 + c[1] as u32 + c[2] as u32) / 3) as f32 * 0.45 + 15.0;
            [g as u8, g as u8, g as u8, c[3]]
        })
        .collect();
    let Some(site) = w.site.as_ref() else { return img };
    let k = px as f32 / win.2;
    let mut dot = |x: f32, z: f32, col: [f32; 3], r: i32| {
        let (cx, cz) = (((x - win.0) * k) as i32, ((z - win.1) * k) as i32);
        for dz in -r..=r {
            for dx in -r..=r {
                let (ix, iz) = (cx + dx, cz + dz);
                if ix >= 0 && iz >= 0 && (ix as usize) < px && (iz as usize) < px {
                    let o = (iz as usize * px + ix as usize) * 4;
                    img[o..o + 3].copy_from_slice(&[col[0] as u8, col[1] as u8, col[2] as u8]);
                }
            }
        }
    };
    // Bigger dots the closer the map is zoomed in, trees bigger than herbs.
    let scale = (k * 1.5).max(1.0) as i32;
    for t in &w.trees {
        let Some(plant) = w.lib.plants.get(t.plant as usize) else { continue };
        if t.pos.x < win.0 || t.pos.z < win.1 || t.pos.x > win.0 + win.2 || t.pos.z > win.1 + win.2 {
            continue;
        }
        let big = crate::world::flora::layer_of(plant) <= 1;
        match plant.site.as_ref() {
            Some(prefs) => {
                let prefs = crate::library::SitePrefs { shade: None, ..prefs.clone() };
                let f = site.suits(&prefs, t.pos.x, t.pos.z, 0.0);
                dot(t.pos.x, t.pos.z, fit_colour(f), if big { scale } else { scale / 2 });
            }
            None => dot(t.pos.x, t.pos.z, [120.0, 120.0, 130.0], scale / 2),
        }
    }
    img
}

/// The plants over the map's ground (biome colours, not greyed): each a grey
/// dot where the biome it stands in grows it, magenta where it came over a
/// border from the neighbouring biome.
fn border_image(ground: &[u8], w: &crate::world::fields::World, win: Win, px: usize) -> Vec<u8> {
    let mut img = ground.to_vec();
    let k = px as f32 / win.2;
    let scale = (k * 1.5).max(1.0) as i32;
    let grows: Vec<Vec<usize>> = w.lib.biomes.iter().map(|b| b.plants.iter().filter_map(|u| w.lib.plant(&u.plant)).collect()).collect();
    for t in &w.trees {
        if (t.plant as usize) >= w.lib.plants.len() || t.pos.x < win.0 || t.pos.z < win.1 || t.pos.x > win.0 + win.2 || t.pos.z > win.1 + win.2 {
            continue;
        }
        let b = w.f.biome[w.f.idx(t.pos.x, t.pos.z)] as usize;
        let own = grows.get(b).is_some_and(|g| g.contains(&(t.plant as usize)));
        let col: [u8; 3] = if own { [150, 150, 150] } else { [255, 60, 220] };
        let big = crate::world::flora::layer_of(&w.lib.plants[t.plant as usize]) <= 1;
        let r = if big { scale } else { scale / 2 };
        let (cx, cz) = (((t.pos.x - win.0) * k) as i32, ((t.pos.z - win.1) * k) as i32);
        for dz in -r..=r {
            for dx in -r..=r {
                let (ix, iz) = (cx + dx, cz + dz);
                if ix >= 0 && iz >= 0 && (ix as usize) < px && (iz as usize) < px {
                    let o = (iz as usize * px + ix as usize) * 4;
                    img[o..o + 3].copy_from_slice(&col);
                }
            }
        }
    }
    img
}

/// A realm's colour on the realm layer.
fn realm_colour(name: &str) -> [u8; 3] {
    match name {
        "bloom" => [90, 230, 255],
        "fae" => [200, 110, 255],
        "dinosaur" => [150, 200, 60],
        "ice_age" => [210, 230, 255],
        "real" => [120, 120, 120],
        _ => [255, 200, 80],
    }
}

/// The realm overlays over the ground: each 4 m cell tinted by its
/// strongest overlay realm, by its strength.
fn realm_image(ground: &[u8], site: &crate::world::site::Site, win: Win, px: usize) -> Vec<u8> {
    let mut img = ground.to_vec();
    let m = &site.realms;
    let colours: Vec<[u8; 3]> = m.names.iter().map(|n| realm_colour(n)).collect();
    for iz in 0..px {
        for ix in 0..px {
            let x = win.0 + (ix as f32 + 0.5) / px as f32 * win.2;
            let z = win.1 + (iz as f32 + 0.5) / px as f32 * win.2;
            let Some((r, st)) = m.strongest(m.idx(x, z)) else { continue };
            let c = colours[r as usize];
            let a = 0.25 + 0.7 * st;
            let o = (iz * px + ix) * 4;
            for j in 0..3 {
                img[o + j] = (img[o + j] as f32 * (1.0 - a) + c[j] as f32 * a) as u8;
            }
        }
    }
    img
}

/// A survey value as the legend says it.
fn survey_value(c: Chan, v: f32) -> String {
    match c.scale() {
        Scale::Flag => if v >= 0.5 { "YES" } else { "NO" }.to_string(),
        Scale::Share => format!("{v:.2}"),
        Scale::Distance if v >= 255.0 * survey::CELL_M => format!("{v:.0} M OR MORE"),
        Scale::Distance => format!("{v:.0} M"),
        Scale::Rise => format!("{v:+.0} M"),
        Scale::Class => c.classes().get(v as usize).copied().unwrap_or("?").to_string(),
    }
}

/// One think, as the step key runs it: the plan's 4 Hz think rate.
const THINK_STEPS: u32 = 15;

#[derive(Default)]
pub struct Eco {
    /// The debug view is on (backquote): the keys below only act while it is.
    pub on: bool,
    /// Whether it froze the animals, so turning it off lets them go again.
    froze: bool,
    /// The ecology map is open (P).
    pub map: bool,
    pub layer: Layer,
    /// The animal the inspector follows.
    pub pinned: Option<hecs::Entity>,
    /// A species name to pin the nearest of once one is about (`--eco-pin`).
    pub pin_wanted: Option<String>,
    /// Steps left to run while the animals are frozen (`.`).
    pub step: u32,
    /// Animal steps run per game step (`,`): 1, or 10.
    pub speed: u32,
    /// The map's ground went up to the page image; cleared when it closes.
    map_sent: bool,
    /// The ground under the map for the window it was drawn for.
    ground: Option<(Win, Vec<u8>)>,
    /// The map's view: the world point at its middle (None: the world's
    /// middle) and how far in it is (1 shows the whole world).
    pub centre: Option<(f32, f32)>,
    pub zoom: f32,
    /// Where the mouse is, screen pixels, while the map has it.
    pub cursor: Option<(f32, f32)>,
    /// A left drag panning the map: where it started on screen, and the
    /// view's middle then.
    drag: Option<((f32, f32), (f32, f32))>,
    /// Where the map was last drawn on screen (x0, y0, side), for the mouse.
    rect: std::cell::Cell<(f32, f32, f32)>,
    /// The window the page image was last drawn for, and when (wheel and
    /// drag redraw at most ten times a second).
    sent: Option<(Win, std::time::Instant)>,
}

/// How an animal shows on the map.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Calm,
    Alert,
    Fleeing,
    Hunting,
    Display,
    /// Not a walker: birds, fish, insects, snakes. Today's spawning round
    /// the player keeps these.
    Ambient,
}

const STATES: [State; 6] = [State::Calm, State::Alert, State::Fleeing, State::Hunting, State::Display, State::Ambient];

impl State {
    fn of(crit: Option<&Critter>, shown: bool) -> State {
        let Some(c) = crit else { return State::Ambient };
        if shown {
            return State::Display;
        }
        match c.mood {
            Mood::Flee => State::Fleeing,
            Mood::Chase | Mood::Feed | Mood::Eat => State::Hunting,
            _ if c.alarm > 0.2 => State::Alert,
            _ => State::Calm,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            State::Calm => "calm",
            State::Alert => "alert",
            State::Fleeing => "fleeing",
            State::Hunting => "hunting",
            State::Display => "display",
            State::Ambient => "ambient",
        }
    }

    fn colour(self) -> [f32; 4] {
        match self {
            State::Calm => [0.35, 0.95, 0.4, 1.0],
            State::Alert => [1.0, 0.85, 0.2, 1.0],
            State::Fleeing => [1.0, 0.25, 0.2, 1.0],
            State::Hunting => [1.0, 0.35, 0.95, 1.0],
            State::Display => [0.3, 0.9, 1.0, 1.0],
            State::Ambient => [0.75, 0.75, 0.8, 0.8],
        }
    }
}

/// The rings round the player on the map: radius, colour, legend line.
fn rings() -> [(f32, [f32; 4], String); 5] {
    let far = spawn::FAR_START;
    [
        (spawn::SPAWN_M, [1.0, 1.0, 1.0, 0.9], format!("SPAWN {:.0} M", spawn::SPAWN_M)),
        (spawn::DROP_M, [0.6, 0.6, 0.65, 0.9], format!("DROP OUT OF VIEW {:.0} M", spawn::DROP_M)),
        (spawn::JOIN_M, [0.45, 0.65, 1.0, 0.9], format!("FAR BAND JOINS {:.0} M", spawn::JOIN_M)),
        (far[0].1, [1.0, 0.6, 0.25, 0.9], format!("FAR BAND FROM {:.0}-{:.0} M BY SIZE", far[0].1, far[far.len() - 1].1)),
        (spawn::DROP_FAR_SEEN_M, [1.0, 0.45, 0.2, 0.9], format!("FAR BAND DROPS {:.0} M", spawn::DROP_FAR_SEEN_M)),
    ]
}

/// The whole world's ground as RGBA rows, `px` square: biome colours with
/// hill shading, the sea by depth and the rivers, as the minimap draws them.
/// world/atlas.rs draws more, but from fields a loaded game no longer keeps.
fn ground_image(w: &crate::world::fields::World, win: Win, px: usize) -> Vec<u8> {
    let f = &w.f;
    let step = win.2 / px as f32;
    let mut img = vec![0u8; px * px * 4];
    for y in 0..px {
        for x in 0..px {
            let (wx, wz) = (win.0 + (x as f32 + 0.5) * step, win.1 + (y as f32 + 0.5) * step);
            let i = f.idx(wx, wz);
            let h = f.h[i];
            let mut c = w.lib.biomes.get(f.biome[i] as usize).and_then(|b| b.map_color).unwrap_or([128.0, 128.0, 128.0]);
            if h < f.sea {
                let d = ((f.sea - h) / 12.0).min(1.0);
                c = [40.0 + 50.0 * (1.0 - d), 90.0 + 60.0 * (1.0 - d), 140.0 + 40.0 * (1.0 - d)];
            } else {
                let sh = (1.0 + ((h - f.height(wx + step, wz)) + (h - f.height(wx, wz + step))) * 0.5 / step.max(0.5)).clamp(0.55, 1.35);
                c = [c[0] * sh, c[1] * sh, c[2] * sh];
            }
            match f.water.get(i).copied().unwrap_or(0) {
                1 => c = [70.0, 130.0, 190.0],
                2 => c = [170.0, 150.0, 120.0],
                3 => c = [150.0, 135.0, 110.0],
                _ => {}
            }
            // Dimmed so the dots stand out on it.
            let k = (y * px + x) * 4;
            for j in 0..3 {
                img[k + j] = (c[j] * 0.8).clamp(0.0, 255.0) as u8;
            }
            img[k + 3] = 255;
        }
    }
    img
}

/// A direction from walk.rs's heading convention (atan2 of x over z).
fn heading_dir(h: f32) -> Vec3 {
    Vec3::new(h.sin(), 0.0, h.cos())
}

impl Game {
    /// Backquote: the debug view on or off. Off closes the map, lets the
    /// pinned animal go and puts the animals back on their own clock.
    pub fn eco_toggle(&mut self) {
        if !self.eco.on {
            self.eco.on = true;
            self.message = Some(("ANIMAL DEBUG ON".to_string(), 1.5));
            return;
        }
        if self.eco.map {
            self.eco_toggle_map();
        }
        if let Some(e) = self.eco.pinned.take() {
            let _ = self.ents.world.remove_one::<Inspect>(e);
        }
        if std::mem::take(&mut self.eco.froze) {
            self.freeze_creatures = false;
        }
        self.eco.step = 0;
        self.eco.speed = 1;
        self.eco.on = false;
        self.message = Some(("ANIMAL DEBUG OFF".to_string(), 1.5));
    }

    /// P: open or close the ecology map.
    pub fn eco_toggle_map(&mut self) {
        if !self.eco.on {
            return;
        }
        self.eco.map = !self.eco.map;
        if !self.eco.map {
            // The map's ground took the page image: a scrapbook page drawn
            // there before is drawn again when next asked for.
            self.eco.map_sent = false;
            self.camera.page_dirty = true;
        }
    }

    /// The map's ground for the page image, once each time the map opens.
    pub(super) fn eco_map_image(&mut self) -> Option<Vec<u8>> {
        if !self.eco.map || self.eco.map_sent {
            return None;
        }
        let w = self.generator().world.clone()?;
        let win = self.eco_window(w.f.size);
        // While the view is moving, at most ten new pictures a second.
        if let Some((last, at)) = self.eco.sent {
            if last != win && at.elapsed().as_secs_f32() < 0.1 {
                return None;
            }
        }
        self.eco.map_sent = true;
        self.eco.sent = Some((win, std::time::Instant::now()));
        let px = crate::render::hud::IMAGE_PX;
        if self.eco.ground.as_ref().is_none_or(|g| g.0 != win) {
            self.eco.ground = Some((win, ground_image(&w, win, px)));
        }
        let ground = &self.eco.ground.as_ref()?.1;
        match (self.eco.layer, w.survey.as_ref()) {
            (Layer::Survey(c), Some(s)) => Some(survey_image(ground, s, c, win, px)),
            (Layer::Site(c), _) if w.site.is_some() => Some(site_image(ground, w.site.as_ref()?, c, win, px)),
            (Layer::Fit, _) => Some(fit_image(ground, &w, win, px)),
            (Layer::Border, _) => Some(border_image(ground, &w, win, px)),
            (Layer::Realm, _) if w.site.is_some() => Some(realm_image(ground, w.site.as_ref()?, win, px)),
            (Layer::Live(c), _) => Some(live_image(ground, &self.ents.live, c, win, px)),
            (Layer::Graph(c), Some(s)) => Some(graph_image(ground, s, c, win, px)),
            _ => Some(ground.clone()),
        }
    }

    /// The part of a world `size` metres across the map shows now.
    pub fn eco_window(&self, size: f32) -> Win {
        let zoom = self.eco.zoom.clamp(1.0, MAX_ZOOM);
        let span = size / zoom;
        let (cx, cz) = self.eco.centre.unwrap_or((size * 0.5, size * 0.5));
        let c = |v: f32| (v - span * 0.5).clamp(0.0, size - span);
        (c(cx), c(cz), span)
    }

    /// The world point under a screen point, if it is on the map.
    fn eco_world_at(&self, (sx, sy): (f32, f32)) -> Option<(f32, f32)> {
        let size = self.generator().world.as_ref()?.f.size;
        let (x0, y0, side) = self.eco.rect.get();
        if side <= 0.0 || sx < x0 || sy < y0 || sx >= x0 + side || sy >= y0 + side {
            return None;
        }
        let (wx, wz, span) = self.eco_window(size);
        Some((wx + (sx - x0) / side * span, wz + (sy - y0) / side * span))
    }

    /// The mouse moved over the screen while the map is open: note it, and
    /// pan if a drag is on.
    pub fn eco_mouse_move(&mut self, x: f32, y: f32) {
        self.eco.cursor = Some((x, y));
        let Some(((sx, sy), (cx, cz))) = self.eco.drag else { return };
        let Some(size) = self.generator().world.as_ref().map(|w| w.f.size) else { return };
        let span = self.eco_window(size).2;
        let side = self.eco.rect.get().2.max(1.0);
        let k = span / side;
        // Kept inside the world so a drag past the edge does not wind up.
        let half = span * 0.5;
        let c = |v: f32| v.clamp(half, size - half);
        self.eco.centre = Some((c(cx - (x - sx) * k), c(cz - (y - sy) * k)));
        self.eco.map_sent = false;
    }

    /// A mouse button while the map is open: left drags the map, right puts
    /// the view back to the whole world. True when the map took it.
    pub fn eco_mouse_button(&mut self, left: bool, down: bool) -> bool {
        if !self.eco.map {
            return false;
        }
        if left {
            if down {
                if let (Some(c), Some(_)) = (self.eco.cursor, self.eco.cursor.and_then(|c| self.eco_world_at(c))) {
                    let size = self.generator().world.as_ref().map_or(0.0, |w| w.f.size);
                    let (wx, wz, span) = self.eco_window(size);
                    self.eco.drag = Some((c, (wx + span * 0.5, wz + span * 0.5)));
                }
            } else {
                self.eco.drag = None;
            }
        } else if down {
            self.eco.zoom = 1.0;
            self.eco.centre = None;
            self.eco.map_sent = false;
        }
        true
    }

    /// The wheel over the map: zoom in (up) or out, about the point under the
    /// mouse, so it stays under the mouse.
    pub fn eco_wheel(&mut self, d: f32) {
        let Some(size) = self.generator().world.as_ref().map(|w| w.f.size) else { return };
        let under = self.eco.cursor.and_then(|c| self.eco_world_at(c));
        let old = self.eco_window(size);
        let zoom = (self.eco.zoom.max(1.0) * 1.25f32.powf(d.clamp(-4.0, 4.0))).clamp(1.0, MAX_ZOOM);
        let span = size / zoom;
        let (x0, y0, side) = self.eco.rect.get();
        let centre = match (under, self.eco.cursor) {
            (Some((wx, wz)), Some((sx, sy))) if side > 0.0 => {
                let (fx, fz) = ((sx - x0) / side, (sy - y0) / side);
                (wx - (fx - 0.5) * span, wz - (fz - 0.5) * span)
            }
            _ => (old.0 + old.2 * 0.5, old.1 + old.2 * 0.5),
        };
        self.eco.zoom = zoom;
        self.eco.centre = Some(centre);
        self.eco.map_sent = false;
    }

    /// Draw the map's layer again on the next frame (the live grid ticked).
    pub(super) fn eco_redraw(&mut self) {
        if self.eco.map {
            self.eco.map_sent = false;
        }
    }

    /// `[` and `]` with the map open: the layer before or after this one.
    pub fn eco_layer(&mut self, by: i32) {
        if !self.eco.on || !self.eco.map {
            return;
        }
        let all = Layer::all();
        let at = all.iter().position(|l| *l == self.eco.layer).unwrap_or(0) as i32;
        self.eco.layer = all[(at + by).rem_euclid(all.len() as i32) as usize];
        // The page image carries the layer: send it again.
        self.eco.map_sent = false;
    }

    /// I: pin the animal under the crosshair, or let the pinned one go.
    pub fn eco_pin(&mut self) {
        if !self.eco.on {
            return;
        }
        let under = self.looking_doing.as_ref().map(|d| d.1);
        let was = self.eco.pinned.take();
        if let Some(e) = was {
            let _ = self.ents.world.remove_one::<Inspect>(e);
        }
        let msg = match under {
            Some(e) if Some(e) != was => {
                self.eco_pin_one(e);
                "PINNED"
            }
            _ if was.is_some() => "UNPINNED",
            _ => "PIN: NO ANIMAL UNDER THE CROSSHAIR",
        };
        self.message = Some((msg.to_string(), 1.5));
    }

    fn eco_pin_one(&mut self, e: hecs::Entity) {
        if self.ents.world.insert_one(e, Inspect::default()).is_ok() {
            self.eco.pinned = Some(e);
        }
    }

    /// O: hold the animals still, or let them go.
    pub fn eco_freeze(&mut self) {
        if !self.eco.on {
            return;
        }
        self.freeze_creatures = !self.freeze_creatures;
        self.eco.froze = self.freeze_creatures;
        self.eco.step = 0;
        self.message = Some((if self.freeze_creatures { "ANIMALS FROZEN" } else { "ANIMALS RUNNING" }.to_string(), 1.5));
    }

    /// `.`: with the animals frozen, run one think of them.
    pub fn eco_step(&mut self) {
        if !self.eco.on {
            return;
        }
        if !self.freeze_creatures {
            self.freeze_creatures = true;
            self.eco.froze = true;
        }
        self.eco.step = THINK_STEPS;
    }

    /// `,`: the animals at ten times speed, or back to one.
    pub fn eco_speed(&mut self) {
        if !self.eco.on {
            return;
        }
        self.eco.speed = if self.eco.speed > 1 { 1 } else { 10 };
        self.message = Some((format!("ANIMALS AT {}X", self.eco.speed), 1.5));
    }

    /// How many animal steps to run this game step: none while frozen
    /// (unless a think is being stepped), else the speed.
    pub(super) fn eco_runs(&mut self) -> u32 {
        if self.freeze_creatures {
            if self.eco.step == 0 {
                return 0;
            }
            self.eco.step -= 1;
            return 1;
        }
        self.eco.speed.max(1)
    }

    /// After the animals step: drop a pin whose animal has gone, and take
    /// up a pin asked for by name (`--eco-pin`).
    pub(super) fn eco_after_step(&mut self) {
        if let Some(e) = self.eco.pinned {
            if !self.ents.world.contains(e) {
                self.eco.pinned = None;
                self.message = Some(("PINNED ANIMAL GONE".to_string(), 2.0));
            }
        }
        let Some(want) = self.eco.pin_wanted.clone().filter(|_| self.eco.pinned.is_none()) else { return };
        let p = self.player.pos;
        let mut best: Option<(f32, hecs::Entity)> = None;
        for (e, look, c, _) in self.ents.world.query::<(hecs::Entity, &Look, &Creature, &Critter)>().iter() {
            let name = &self.ents.species[c.species].name;
            if want != "nearest" && !name.contains(want.as_str()) {
                continue;
            }
            let d = (look.pos - p).length();
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, e));
            }
        }
        if let Some((_, e)) = best {
            self.eco_pin_one(e);
            self.eco.pin_wanted = None;
        }
    }

    /// Where the pinned animal is, if it is still about.
    pub fn eco_pinned_pos(&self) -> Option<Vec3> {
        let e = self.eco.pinned?;
        self.ents.world.get::<&Look>(e).ok().map(|l| l.pos)
    }

    /// The whole world, north up, with every animal on it coloured by what
    /// it is doing and the spawn rings round the player (D1).
    /// Returns where the legend ends (x, y), for the inspector to go under.
    pub(super) fn build_eco_map(&self, hud: &mut Hud, w: f32, h: f32, s: f32) -> (f32, f32) {
        let dim = [0.85, 0.9, 0.95, 0.85];
        let white = [1.0, 1.0, 1.0, 0.95];
        let line = 10.0 * s;
        hud.rect(0.0, 0.0, w, h, [0.02, 0.03, 0.04, 0.92]);
        let Some(world) = self.generator().world.as_ref() else {
            hud.label(24.0, 24.0, s * 1.5, white, "NO MAP: THIS WORLD HAS NO FIELDS");
            return (24.0, 24.0 + line * 3.0);
        };
        let size = world.f.size;
        let m = (16.0 * s).floor();
        let side = (h - 2.0 * m).min(w * 0.62).floor();
        let (x0, y0) = (m, ((h - side) * 0.5).floor());
        self.eco.rect.set((x0, y0, side));
        hud.page(x0, y0, side, side);
        let (vx, vz, span) = self.eco_window(size);
        let k = side / span;
        let at = |p: Vec3| (x0 + (p.x - vx) * k, y0 + (p.z - vz) * k);
        let inside = |(sx, sy): (f32, f32)| sx >= x0 && sy >= y0 && sx < x0 + side && sy < y0 + side;

        // The rings, as dots round the player.
        let p = self.player.pos;
        for (r, c, _) in rings() {
            let n = ((r * k * 0.6) as usize).clamp(48, 400);
            for i in 0..n {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let q = at(p + Vec3::new(a.cos(), 0.0, a.sin()) * r);
                if inside(q) {
                    hud.rect(q.0 - 1.0, q.1 - 1.0, 2.0, 2.0, c);
                }
            }
        }

        // The animals: walkers larger, distant ones ringed in black.
        let mut counts = [0usize; STATES.len()];
        let mut herds = crate::util::FxHashSet::default();
        let world_e = &self.ents.world;
        let mut q = world_e.query::<(
            hecs::Entity,
            &Look,
            &Creature,
            Option<&Critter>,
            Option<&crate::creatures::display::Show>,
            Option<&spawn::Distant>,
            Option<&Kin>,
        )>();
        let mut pinned_at = None;
        for (e, look, _, crit, show, distant, kin) in q.iter() {
            let st = State::of(crit, show.is_some());
            counts[st as usize] += 1;
            if let Some(k) = kin {
                herds.insert(k.group);
            }
            let (sx, sy) = at(look.pos);
            if !inside((sx, sy)) {
                continue;
            }
            let r = if st == State::Ambient { (1.0 * s).max(1.5) } else { (1.6 * s).max(2.5) };
            if distant.is_some() {
                hud.rect(sx - r - 1.0, sy - r - 1.0, r * 2.0 + 2.0, r * 2.0 + 2.0, [0.0, 0.0, 0.0, 0.9]);
            }
            hud.rect(sx - r, sy - r, r * 2.0, r * 2.0, st.colour());
            if Some(e) == self.eco.pinned {
                pinned_at = Some((sx, sy));
            }
        }
        drop(q);
        if let Some((sx, sy)) = pinned_at {
            let r = 5.0 * s;
            for (dx, dy, ww, hh) in [(-r, -r, 2.0 * r, 1.5), (-r, r - 1.5, 2.0 * r, 1.5), (-r, -r, 1.5, 2.0 * r), (r - 1.5, -r, 1.5, 2.0 * r)] {
                hud.rect(sx + dx, sy + dy, ww, hh, white);
            }
        }

        // The survey graph's anchors and zone names.
        if let (Layer::Graph(_), Some(sv)) = (self.eco.layer, world.survey.as_ref()) {
            for z in sv.graph.zones.iter() {
                let (zx, zy) = at(Vec3::new(z.centre[0], 0.0, z.centre[1]));
                let ls = s * if z.named { 0.7 } else { 0.55 };
                let name = z.name.to_uppercase().replace('_', " ");
                let tw = Hud::text_width(&name, ls);
                if inside((zx - tw * 0.5, zy)) && inside((zx + tw * 0.5, zy + 10.0 * ls)) {
                    hud.label((zx - tw * 0.5).floor(), zy.floor(), ls, [1.0, 1.0, 1.0, if z.named { 0.9 } else { 0.55 }], &name);
                }
            }
            for a in &sv.graph.anchors {
                let (ax, ay) = at(Vec3::new(a.pos[0], 0.0, a.pos[2]));
                if inside((ax, ay)) {
                    let r = (1.8 * s).max(2.5);
                    hud.rect(ax - r - 1.0, ay - r - 1.0, r * 2.0 + 2.0, r * 2.0 + 2.0, [0.0, 0.0, 0.0, 0.9]);
                    hud.rect(ax - r, ay - r, r * 2.0, r * 2.0, kind_colour(a.kind));
                }
            }
        }

        // Herds kept as records and not drawn now (creatures/roster.rs): a
        // hollow square where each one's middle is, red for a pack or a lone
        // hunter.
        for hd in self.ents.roster.herds.iter().filter(|h| !h.drawn) {
            let q = at(hd.centre);
            if inside(q) {
                let r = (1.5 + (hd.size as f32).sqrt()) * s.max(1.0);
                let c = if self.ents.species[hd.species].hunts.is_empty() { [0.95, 0.95, 0.75, 0.8] } else { [1.0, 0.3, 0.25, 0.9] };
                for (dx, dy, ww, hh) in [(-r, -r, 2.0 * r, 1.0), (-r, r - 1.0, 2.0 * r, 1.0), (-r, -r, 1.0, 2.0 * r), (r - 1.0, -r, 1.0, 2.0 * r)] {
                    hud.rect(q.0 + dx, q.1 + dy, ww, hh, c);
                }
            }
        }

        // Kills made out of sight (creatures/roster.rs `encounters`): a red
        // cross where each one lies.
        for k in &self.ents.roster.kills {
            let q = at(k.at);
            if inside(q) {
                let r = 6.0 * s.max(1.0);
                for j in -6..=6 {
                    let f = j as f32 / 6.0 * r;
                    hud.rect(q.0 + f - 1.5, q.1 + f - 1.5, 3.0, 3.0, [1.0, 0.1, 0.1, 1.0]);
                    hud.rect(q.0 + f - 1.5, q.1 - f - 1.5, 3.0, 3.0, [1.0, 0.1, 0.1, 1.0]);
                }
            }
        }

        // Where each herd is going (creatures/roam.rs): the rest of its way as
        // dots in the colour of its need, and a ring on its goal.
        for (_, centre, _, t) in self.ents.roam.trips() {
            let col = need_colour(t.need);
            let mut last = at(centre);
            for q in t.route.iter().skip(t.at) {
                let next = at(*q);
                let steps = (((next.0 - last.0).powi(2) + (next.1 - last.1).powi(2)).sqrt() / (5.0 * s).max(4.0)).ceil().max(1.0) as usize;
                for j in 1..=steps {
                    let f = j as f32 / steps as f32;
                    let d = (last.0 + (next.0 - last.0) * f, last.1 + (next.1 - last.1) * f);
                    if inside(d) {
                        hud.rect(d.0 - 1.0, d.1 - 1.0, 2.0, 2.0, col);
                    }
                }
                last = next;
            }
            if let Some(q) = t.route.last() {
                let g = at(*q);
                if inside(g) {
                    let r = 5.0 * s;
                    for (dx, dy, ww, hh) in [(-r, -r, 2.0 * r, 1.5), (-r, r - 1.5, 2.0 * r, 1.5), (-r, -r, 1.5, 2.0 * r), (r - 1.5, -r, 1.5, 2.0 * r)] {
                        hud.rect(g.0 + dx, g.1 + dy, ww, hh, col);
                    }
                }
            }
        }

        // The player, with a stalk the way they face.
        let (mx, my) = at(p);
        if inside((mx, my)) {
            let fv = self.player.forward();
            let fl = Vec3::new(fv.x, 0.0, fv.z).normalize_or_zero();
            for i in 1..=6 {
                let d = i as f32 * 1.6 * s;
                hud.rect(mx + fl.x * d - 1.0, my + fl.z * d - 1.0, 2.0, 2.0, [1.0, 0.95, 0.9, 0.95]);
            }
            let r = (2.0 * s).ceil();
            hud.rect(mx - r - 1.0, my - r - 1.0, r * 2.0 + 2.0, r * 2.0 + 2.0, [0.0, 0.0, 0.0, 0.9]);
            hud.rect(mx - r, my - r, r * 2.0, r * 2.0, [1.0, 0.3, 0.25, 1.0]);
        }
        hud.label(x0 + 4.0, y0 + 4.0, s * 0.9, white, "N");

        // The legend, right of the map.
        let lx = x0 + side + 2.0 * m;
        let mut y = y0;
        hud.label(lx, y, s * 1.4, white, "ECOLOGY");
        y += line * 1.8;
        let speed = if self.freeze_creatures { "FROZEN".to_string() } else { format!("{}X", self.eco.speed.max(1)) };
        let all = Layer::all();
        let at = all.iter().position(|l| *l == self.eco.layer).unwrap_or(0);
        for l in [
            format!("LAYER {}  ({} OF {})  {:.0} M ACROSS, ZOOM {:.1}X", self.eco.layer.name().to_uppercase(), at + 1, all.len(), span, size / span),
            format!("{:02}:{:02}  ANIMALS {speed}", self.hour.floor(), (self.hour.fract() * 60.0) as u32),
            format!("{} ANIMALS  {} HERDS", counts.iter().sum::<usize>(), herds.len()),
            {
                let r = &self.ents.roster;
                let drawn = r.herds.iter().filter(|h| h.drawn).count();
                let hunters = r.herds.iter().filter(|h| !self.ents.species[h.species].hunts.is_empty()).count();
                format!("{} RECORDS, {hunters} OF THEM PACKS AND LONE HUNTERS IN RED ({drawn} DRAWN, HOLLOW SQUARES THE REST), {} LOST", r.herds.len(), r.lost)
            },
        ] {
            hud.label(lx, y, s, dim, &l);
            y += line;
        }
        y += line * 0.6;
        let sw = 6.0 * s;
        if let Layer::Survey(c) = self.eco.layer {
            y = self.survey_legend(hud, world, c, lx, y, s);
        }
        if let Layer::Site(c) = self.eco.layer {
            y = self.site_legend(hud, world, c, lx, y, s);
        }
        if self.eco.layer == Layer::Realm {
            hud.label(lx, y, s, [1.0, 1.0, 1.0, 0.95], "REALMS COMING THROUGH (STRONGEST, BRIGHTER STRONGER)");
            y += line;
            if let Some(site) = world.site.as_ref() {
                for name in site.realms.names.iter().skip(1) {
                    let c = realm_colour(name);
                    hud.label(lx, y, s * 0.85, [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 0.95], &name.to_uppercase());
                    y += line * 0.9;
                }
            }
            y += line * 0.6;
        }
        if self.eco.layer == Layer::Border {
            hud.label(lx, y, s, [1.0, 1.0, 1.0, 0.95], "PLANTS ACROSS BIOME BORDERS");
            y += line;
            hud.label(lx, y, s * 0.85, [0.85, 0.9, 0.95, 0.85], "GREY: ITS BIOME GROWS IT  MAGENTA: FROM THE NEIGHBOUR");
            y += line * 1.6;
        }
        if self.eco.layer == Layer::Fit {
            let placing = if crate::world::flora::placement_v2(world) { "V2, BY SITE" } else { "V1, BY LIST" };
            hud.label(lx, y, s, [1.0, 1.0, 1.0, 0.95], &format!("PLANTS BY HOW WELL THEIR GROUND SUITS THEM ({placing})"));
            y += line;
            hud.label(lx, y, s * 0.85, [0.85, 0.9, 0.95, 0.85], "SHADE LEFT OUT; TREES LARGER; GREY: NO SITE BLOCK");
            y += line * 1.3;
            for v in [1.0f32, 0.5, 0.0] {
                let c = fit_colour(v);
                hud.rect(lx, y + 1.0, 6.0 * s, 6.0 * s, [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]);
                hud.label(lx + 14.0 * s, y, s, [0.85, 0.9, 0.95, 0.85], &format!("{v:.2}"));
                y += line;
            }
            y += line * 0.6;
        }
        if let Layer::Live(c) = self.eco.layer {
            y = self.live_legend(hud, c, lx, y, s);
        }
        if let Layer::Graph(c) = self.eco.layer {
            y = self.graph_legend(hud, world, c, lx, y, s);
        }
        for st in STATES {
            hud.rect(lx, y + 1.0, sw, sw, st.colour());
            hud.label(lx + sw + 8.0 * s, y, s, dim, &format!("{}  {}", st.name().to_uppercase(), counts[st as usize]));
            y += line;
        }
        hud.rect(lx - 1.0, y, sw + 2.0, sw + 2.0, [0.0, 0.0, 0.0, 0.9]);
        hud.rect(lx, y + 1.0, sw, sw, [0.6, 0.6, 0.6, 1.0]);
        hud.label(lx + sw + 8.0 * s, y, s, dim, "RINGED: FAR BAND (DISTANT)");
        y += line * 1.6;
        for (_, c, l) in rings() {
            for i in 0..3 {
                hud.rect(lx + i as f32 * 2.5 * s, y + sw * 0.5, 1.5 * s, 1.5 * s, c);
            }
            hud.label(lx + sw + 8.0 * s, y, s, dim, &l);
            y += line;
        }
        y += line * 0.6;
        hud.label(lx, y, s * 0.85, dim, "P CLOSE  [ ] LAYER  I PIN  O FREEZE  . STEP  , 10X  ` OFF");
        y += line;
        hud.label(lx, y, s * 0.85, dim, "WHEEL ZOOM  LEFT DRAG PAN  RIGHT CLICK WHOLE WORLD  HOVER FOR DETAIL");
        self.eco_hover(hud, world, w, h, s, k);
        (lx, y + line * 2.0)
    }

    /// What is under the mouse on the map, in a box beside it: where, the
    /// ground and zone, this layer's value there, and the nearest anchor and
    /// animal within a few pixels. `k` is screen pixels per metre.
    fn eco_hover(&self, hud: &mut Hud, world: &crate::world::fields::World, w: f32, h: f32, s: f32, k: f32) {
        let Some(c) = self.eco.cursor else { return };
        let Some((wx, wz)) = self.eco_world_at(c) else { return };
        let f = &world.f;
        let i = f.idx(wx, wz);
        let biome = world.lib.biomes.get(f.biome[i] as usize).map_or("-".to_string(), |b| b.name.to_uppercase().replace('_', " "));
        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
        let white = [1.0, 1.0, 1.0, 0.95];
        let dim = [0.85, 0.9, 0.95, 0.85];
        let water = if f.h[i] < f.sea { format!("  {:.1} M UNDER THE SEA", f.sea - f.h[i]) } else { String::new() };
        lines.push((format!("X {wx:.0}  Z {wz:.0}  GROUND {:.1} M{water}", f.h[i]), white));
        lines.push((format!("BIOME {biome}"), dim));
        let sv = world.survey.as_ref();
        if let Some(sv) = sv {
            let zi = sv.graph.zone_of.get(sv.idx(wx, wz)).copied().unwrap_or(0);
            if zi > 0 {
                let z = &sv.graph.zones[zi as usize - 1];
                lines.push((format!("ZONE {}  ({})", z.name.to_uppercase().replace('_', " "), z.tags.join(", ").to_uppercase().replace('_', " ")), dim));
                lines.push((format!("  OPEN {:.2}  COVER {:.2}  FORAGE {:.2}  ENCLOSED {:.2}  {} CELLS", z.open, z.cover, z.forage, z.enclosure, z.cells), dim));
            }
        }
        match self.eco.layer {
            Layer::Survey(c) => {
                if let Some(sv) = sv {
                    lines.push((format!("{}: {}", c.name().to_uppercase(), survey_value(c, sv.value(c, wx, wz))), [1.0, 0.9, 0.5, 1.0]));
                }
            }
            Layer::Site(c) => {
                if let Some(site) = world.site.as_ref() {
                    lines.push((format!("{}: {:.2}", c.name().to_uppercase(), site.value(c, wx, wz)), [1.0, 0.9, 0.5, 1.0]));
                }
            }
            Layer::Live(c) => {
                lines.push((format!("{}: {:.2}", c.name().to_uppercase(), self.ents.live.value(c, wx, wz)), [1.0, 0.9, 0.5, 1.0]));
            }
            Layer::Fit => {
                // The plant nearest the cursor, and how its ground suits it,
                // field by field.
                let reach = 10.0 / k;
                let near = world
                    .trees
                    .iter()
                    .filter(|t| (t.pos.x - wx).abs() < reach && (t.pos.z - wz).abs() < reach)
                    .min_by(|a, b| ((a.pos.x - wx).powi(2) + (a.pos.z - wz).powi(2)).total_cmp(&((b.pos.x - wx).powi(2) + (b.pos.z - wz).powi(2))));
                if let (Some(t), Some(site)) = (near, world.site.as_ref()) {
                    if let Some(plant) = world.lib.plants.get(t.plant as usize) {
                        lines.push((plant.name.to_uppercase().replace('_', " "), [1.0, 0.9, 0.5, 1.0]));
                        match plant.site.as_ref() {
                            Some(prefs) => {
                                for (c, r) in [
                                    (SiteChan::Wet, prefs.wet),
                                    (SiteChan::Soil, prefs.soil),
                                    (SiteChan::Sun, prefs.sun),
                                    (SiteChan::Exposure, prefs.exposure),
                                    (SiteChan::Cold, prefs.cold),
                                    (SiteChan::Rock, prefs.rock),
                                    (SiteChan::Disturbed, prefs.disturbed),
                                ] {
                                    if let Some(r) = r {
                                        let v = site.value(c, t.pos.x, t.pos.z);
                                        lines.push((format!("  {} {v:.2} IN {:.2}-{:.2}: FIT {:.2}", c.name().to_uppercase(), r[0], r[1], crate::world::site::fit(r, v)), dim));
                                    }
                                }
                                if let Some(r) = prefs.shade {
                                    lines.push((format!("  SHADE WANTED {:.2}-{:.2} (CROWN SHADE NOT KEPT)", r[0], r[1]), dim));
                                }
                            }
                            None => lines.push(("  NO SITE BLOCK".to_string(), dim)),
                        }
                    }
                }
            }
            Layer::Graph(_) | Layer::Creatures | Layer::Border | Layer::Realm => {}
        }
        // The nearest anchor within 10 px.
        if let Some(sv) = sv {
            let reach = 10.0 / k;
            let near = sv.graph.anchors.iter().map(|a| (((a.pos[0] - wx).powi(2) + (a.pos[2] - wz).powi(2)).sqrt(), a)).filter(|(d, _)| *d <= reach).min_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, a)) = near {
                let zone = sv.graph.zones.get(a.zone as usize).map_or("-".to_string(), |z| z.name.to_uppercase());
                lines.push((format!("{} ANCHOR IN {zone}", a.kind.name().to_uppercase()), kind_colour(a.kind)));
                lines.push((
                    format!(
                        "  {:+.1} M OVER THE ZONE  OPEN {:.2}  COVER {:.2}  SEEN TO {:.0} M  WAYS IN {}  FLAT {:.0} M{}  WATER {:.0} M",
                        a.above_floor,
                        a.open,
                        a.cover,
                        a.sightline,
                        a.ways_in,
                        a.flat,
                        if a.roofed { "  ROOFED" } else { "" },
                        a.drink
                    ),
                    dim,
                ));
                let links = sv.graph.links.iter().filter(|l| sv.graph.anchors.get(l.a as usize).is_some_and(|x| std::ptr::eq(x, a)) || sv.graph.anchors.get(l.b as usize).is_some_and(|x| std::ptr::eq(x, a)));
                let mut per = [0usize; 3];
                for l in links {
                    per[Class::ALL.iter().position(|&c| c == l.class).unwrap_or(0)] += 1;
                }
                lines.push((format!("  GRAZE {:.2}  BROWSE {:.2} WITHIN 25 M", a.graze, a.browse), dim));
                lines.push((format!("  LINKS: PERSON {}  LARGE {}  VEHICLE {}", per[0], per[1], per[2]), dim));
            }
        }
        // The nearest animal within 8 px.
        let reach = 8.0 / k;
        let mut best: Option<(f32, String)> = None;
        for (look, c, crit, show, kin) in self.ents.world.query::<(&Look, &Creature, Option<&Critter>, Option<&crate::creatures::display::Show>, Option<&Kin>)>().iter() {
            let d = ((look.pos.x - wx).powi(2) + (look.pos.z - wz).powi(2)).sqrt();
            if d <= reach && best.as_ref().is_none_or(|b| d < b.0) {
                let name = crate::creatures::display_name(&self.ents.species, c.species).replace('_', " ").to_uppercase();
                let st = State::of(crit, show.is_some());
                let who = crate::creatures::roster::kind(&self.ents.species[c.species]).to_uppercase();
                let trip = kin.and_then(|k| self.ents.roam.trip(k.group)).map_or(String::new(), |t| {
                    let goal = t.route.last().map_or(0.0, |q| ((q.x - look.pos.x).powi(2) + (q.z - look.pos.z).powi(2)).sqrt());
                    match t.arrived {
                        Some(_) => format!("  {who} AT ITS {} GOAL", t.need.name().to_uppercase()),
                        None => format!("  {who} OFF TO {}, {goal:.0} M", t.need.name().to_uppercase()),
                    }
                });
                let wary = kin.map_or(0.0, |k| self.ents.roam.wary_of(k.group));
                let wary = if wary.abs() >= 0.05 { format!("  WARY {wary:.2}") } else { String::new() };
                best = Some((d, format!("{name}  {}{trip}{wary}", st.name().to_uppercase())));
            }
        }
        if let Some((_, l)) = best {
            lines.push((l, [0.6, 1.0, 0.6, 1.0]));
        }
        let near = self.ents.roster.herds.iter().filter(|h| !h.drawn).map(|h| (((h.centre.x - wx).powi(2) + (h.centre.z - wz).powi(2)).sqrt(), h)).filter(|(d, _)| *d <= 10.0 / k).min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, hd)) = near {
            let name = crate::creatures::display_name(&self.ents.species, hd.species).replace('_', " ").to_uppercase();
            let trip = self.ents.roam.trip(hd.id).map_or("NO TRIP".to_string(), |t| match t.arrived {
                Some(_) => format!("AT ITS {} GOAL", t.need.name().to_uppercase()),
                None => format!("OFF TO {}", t.need.name().to_uppercase()),
            });
            let kept = if hd.members.is_empty() { "NEVER DRAWN" } else { "ANIMALS KEPT" };
            let who = crate::creatures::roster::kind(&self.ents.species[hd.species]).to_uppercase();
            lines.push((format!("{who} RECORD: {} {name}, NOT DRAWN, {trip}, {kept}", hd.size), [0.95, 0.95, 0.75, 1.0]));
        }
        let ls = s * 0.8;
        let lh = 10.0 * ls * 1.1;
        let bw = lines.iter().map(|l| Hud::text_width(&l.0, ls)).fold(0.0, f32::max) + 16.0;
        let bh = lines.len() as f32 * lh + 10.0;
        let (mut bx, mut by) = (c.0 + 18.0, c.1 + 18.0);
        if bx + bw > w {
            bx = (c.0 - 18.0 - bw).max(0.0);
        }
        if by + bh > h {
            by = (c.1 - 18.0 - bh).max(0.0);
        }
        hud.rect(c.0 - 3.0, c.1 - 0.5, 7.0, 1.0, white);
        hud.rect(c.0 - 0.5, c.1 - 3.0, 1.0, 7.0, white);
        hud.rect(bx, by, bw, bh, [0.0, 0.0, 0.0, 0.8]);
        for (j, (l, col)) in lines.iter().enumerate() {
            hud.label(bx + 8.0, by + 5.0 + j as f32 * lh, ls, *col, l);
        }
    }

    /// The graph layer's part of the legend: counts, the audit for this
    /// class, the anchor kinds and their colours, and the zone underfoot.
    fn graph_legend(&self, hud: &mut Hud, world: &crate::world::fields::World, c: Class, lx: f32, mut y: f32, s: f32) -> f32 {
        let line = 10.0 * s;
        let dim = [0.85, 0.9, 0.95, 0.85];
        let Some(sv) = world.survey.as_ref() else {
            hud.label(lx, y, s, [1.0, 0.6, 0.4, 0.95], "NO SURVEY: THIS WORLD IS NOT ONE TO EXPLORE");
            return y + line * 1.6;
        };
        let g = &sv.graph;
        hud.label(lx, y, s, [1.0, 1.0, 1.0, 0.95], &format!("ZONES, ANCHORS AND {} LINKS (RED: THROUGH A PINCH)", c.name().to_uppercase()));
        y += line;
        let p = self.player.pos;
        let here = g.zone_of.get(sv.idx(p.x, p.z)).copied().unwrap_or(0);
        let zone = if here > 0 { g.zones[here as usize - 1].name.to_uppercase() } else { "NONE".to_string() };
        hud.label(lx, y, s, dim, &format!("HERE: {zone}"));
        y += line;
        let audit = g.audit();
        let ci = Class::ALL.iter().position(|&k| k == c).unwrap_or(0);
        for l in [
            format!("{} ZONES ({} NAMED), {} ANCHORS", g.zones.len(), g.zones.iter().filter(|z| z.named).count(), g.anchors.len()),
            audit.get(ci).cloned().unwrap_or_default().to_uppercase(),
        ] {
            hud.label(lx, y, s * 0.85, dim, &l);
            y += line;
        }
        y += line * 0.3;
        let sw = 6.0 * s;
        for k in Kind::ALL {
            let n = g.anchors.iter().filter(|a| a.kind == k).count();
            hud.rect(lx, y + 1.0, sw, sw, kind_colour(k));
            hud.label(lx + sw + 8.0 * s, y, s, dim, &format!("{}  {n}", k.name().to_uppercase()));
            y += line;
        }
        y + line * 0.6
    }

    /// The live layer's part of the legend: what the channel is, how it
    /// keeps, its colours and its level where the player stands.
    fn live_legend(&self, hud: &mut Hud, c: LiveChan, lx: f32, mut y: f32, s: f32) -> f32 {
        use crate::world::live::{Keep, CELL_M, TICK_S};
        let line = 10.0 * s;
        let dim = [0.85, 0.9, 0.95, 0.85];
        let l = &self.ents.live;
        hud.label(lx, y, s, [1.0, 1.0, 1.0, 0.95], c.about());
        y += line;
        let p = self.player.pos;
        hud.label(lx, y, s, dim, &format!("HERE: {:.2}", l.value(c, p.x, p.z)));
        y += line;
        let keep = match c.keep() {
            Keep::Fades => format!("FADES TO HALF IN {:.0} S", c.half_life()),
            Keep::Counted => "COUNTED EACH TICK".to_string(),
            Keep::Lingers => format!("COUNTED, THEN FADES TO HALF IN {:.0} MIN", c.half_life() / 60.0),
        };
        hud.label(lx, y, s * 0.85, dim, &format!("LIVE {CELL_M} M CELLS, TICK {TICK_S} S ({} SO FAR, {:.2} MS)  {keep}", l.ticks, l.tick_ms));
        y += line * 1.3;
        let sw = 6.0 * s;
        for k in [1.0, 0.75, 0.5, 0.25] {
            let col = ramp(k);
            hud.rect(lx, y + 1.0, sw, sw, [col[0] / 255.0, col[1] / 255.0, col[2] / 255.0, 1.0]);
            hud.label(lx + sw + 8.0 * s, y, s, dim, &format!("{:.2}", k * c.full()));
            y += line;
        }
        y + line * 0.6
    }

    /// The survey layer's part of the legend: what the channel is, its
    /// colours, and its value where the player stands. Returns the next y.
    fn survey_legend(&self, hud: &mut Hud, world: &crate::world::fields::World, c: Chan, lx: f32, mut y: f32, s: f32) -> f32 {
        let line = 10.0 * s;
        let dim = [0.85, 0.9, 0.95, 0.85];
        let white = [1.0, 1.0, 1.0, 0.95];
        let Some(sv) = world.survey.as_ref() else {
            hud.label(lx, y, s, [1.0, 0.6, 0.4, 0.95], "NO SURVEY: THIS WORLD IS NOT ONE TO EXPLORE");
            return y + line * 1.6;
        };
        hud.label(lx, y, s, white, c.about());
        y += line;
        let p = self.player.pos;
        hud.label(lx, y, s, dim, &format!("HERE: {}", survey_value(c, sv.value(c, p.x, p.z))));
        y += line;
        hud.label(lx, y, s * 0.85, dim, &format!("SURVEY {} M CELLS, {}", survey::CELL_M, sv.origin.to_uppercase()));
        y += line * 1.3;
        let sw = 6.0 * s;
        let swatch = |hud: &mut Hud, y: f32, b: u8, label: &str| {
            let (col, a) = survey_colour(c, b);
            let col = if a == 0.0 { [90.0, 90.0, 90.0] } else { col };
            hud.rect(lx, y + 1.0, sw, sw, [col[0] / 255.0, col[1] / 255.0, col[2] / 255.0, 1.0]);
            hud.label(lx + sw + 8.0 * s, y, s, dim, label);
        };
        let stops: Vec<(u8, String)> = match c.scale() {
            Scale::Flag => vec![(255, "YES".into()), (0, "NO".into())],
            Scale::Class => c.classes().iter().enumerate().map(|(k, n)| (k as u8, n.to_string())).collect(),
            Scale::Share => [255u8, 191, 128, 64, 1].iter().map(|&b| (b, format!("{:.2}", b as f32 / 255.0))).collect(),
            Scale::Distance => [0u8, 16, 32, 64].iter().map(|&b| (b, survey_value(c, survey::decode(c, b)))).collect(),
            Scale::Rise => {
                let r: i32 = if c == Chan::Rise200 { 40 } else { 12 };
                [r, r / 2, 0, -r / 2, -r].iter().map(|&d| ((128 + d) as u8, format!("{d:+} M"))).collect()
            }
        };
        for (b, l) in stops {
            swatch(hud, y, b, &l);
            y += line;
        }
        y + line * 0.6
    }

    /// A site layer's part of the legend: what the field is, its ramp, and
    /// its value where the player stands. Returns the next y.
    fn site_legend(&self, hud: &mut Hud, world: &crate::world::fields::World, c: SiteChan, lx: f32, mut y: f32, s: f32) -> f32 {
        let line = 10.0 * s;
        let dim = [0.85, 0.9, 0.95, 0.85];
        let white = [1.0, 1.0, 1.0, 0.95];
        let Some(site) = world.site.as_ref() else {
            hud.label(lx, y, s, [1.0, 0.6, 0.4, 0.95], "NO SITE FIELDS IN THIS WORLD");
            return y + line * 1.6;
        };
        hud.label(lx, y, s, white, c.about());
        y += line;
        let p = self.player.pos;
        let k = site.idx(p.x, p.z);
        let biome = |b: u8| world.lib.biomes.get(b as usize).map_or("?", |d| d.name.as_str()).to_uppercase();
        hud.label(lx, y, s, dim, &format!("HERE: {:.2}  IN {}", site.value(c, p.x, p.z), biome(site.biome[k])));
        y += line;
        if c == SiteChan::Soil && site.other[k] != site.biome[k] && site.border[k] < 255 {
            hud.label(lx, y, s, dim, &format!("BORDER WITH {} {} M", biome(site.other[k]), site.border[k]));
            y += line;
        }
        hud.label(lx, y, s * 0.85, dim, &format!("SITE {} M CELLS, BUILT IN {:.0} MS", survey::CELL_M, site.ms()));
        y += line * 1.3;
        let sw = 6.0 * s;
        for v in [1.0f32, 0.75, 0.5, 0.25, 0.0] {
            let col = ramp(v);
            hud.rect(lx, y + 1.0, sw, sw, [col[0] / 255.0, col[1] / 255.0, col[2] / 255.0, 1.0]);
            hud.label(lx + sw + 8.0 * s, y, s, dim, &format!("{v:.2}"));
            y += line;
        }
        y + line * 0.6
    }

    /// While the debug view is on, its keys, top right under the finds.
    pub(super) fn build_eco_keys(&self, hud: &mut Hud, w: f32, s: f32) {
        if !self.eco.on {
            return;
        }
        let state = if self.freeze_creatures { "  FROZEN" } else if self.eco.speed > 1 { "  10X" } else { "" };
        let text = format!("ANIMAL DEBUG{state}  P MAP  I PIN  O FREEZE  . STEP  , 10X  ` OFF");
        let ls = s * 0.85;
        let tw = Hud::text_width(&text, ls);
        let (x, y) = ((w - tw - 20.0).floor(), 16.0 + 10.0 * s * 2.2);
        hud.rect(x - 8.0, y - 5.0, tw + 16.0, 10.0 * ls + 8.0, [0.0, 0.0, 0.0, 0.5]);
        hud.label(x, y, ls, [1.0, 0.85, 0.4, 0.95], &text);
    }

    /// The pinned animal's panel (D2): at the right of the screen, or with
    /// its top left at `at` (under the map's legend).
    pub(super) fn build_inspector(&self, hud: &mut Hud, w: f32, h: f32, s: f32, at: Option<(f32, f32)>) {
        let Some(e) = self.eco.pinned else { return };
        let Ok(ent) = self.ents.world.entity(e) else { return };
        let line = 10.0 * s;
        let mut lines: Vec<(String, [f32; 4])> = Vec::new();
        let white = [1.0, 1.0, 1.0, 0.95];
        let dim = [0.85, 0.9, 0.95, 0.85];
        let quiet = [0.75, 0.8, 0.9, 0.7];
        let sp = ent.get::<&Creature>().map(|c| c.species);
        let name = sp.map_or("animal".to_string(), |s| crate::creatures::display_name(&self.ents.species, s).replace('_', " "));
        let distant = if ent.has::<spawn::Distant>() { "  FAR BAND" } else { "" };
        lines.push((format!("{}  #{}{distant}", name.to_uppercase(), e.id()), white));
        lines.push((crate::creatures::census::doing(&self.ents.world, e), dim));
        let look = ent.get::<&Look>().map(|l| l.pos);
        let dist = look.map_or(0.0, |l| Vec3::new(l.x - self.player.pos.x, 0.0, l.z - self.player.pos.z).length());
        let crit = ent.get::<&Critter>();
        let insp = ent.get::<&Inspect>();
        let life = ent.get::<&crate::anim::Anim>();
        let act = life.as_ref().and_then(|a| a.life.as_ref().map(|l| l.act.name())).unwrap_or("-");
        if let Some(c) = crit.as_deref() {
            lines.push((format!("MOOD {}  ACT {}  TIMER {:.1} S", c.mood.name(), act, c.timer.max(0.0)).to_uppercase(), dim));
            let flee = insp.as_deref().map_or(0.0, |i| i.flee_m);
            lines.push((format!("ALARM {:.2} OF 1  FLEE {:.0} M  PLAYER {:.0} M", c.alarm, flee, dist), dim));
        } else {
            lines.push((format!("NOT A WALKER: NO MOOD OR ALARM  PLAYER {dist:.0} M"), dim));
        }
        if let Some(i) = insp.as_deref() {
            let p = i.player;
            lines.push((format!("PLAYER  SIGHT {:.2}  NOISE {:.2}  SCENT {:.2}  CLOSE {:.2}", p.sight, p.noise, p.scent, p.close), dim));
            lines.push((format!("PREDATOR {:.2}  NEIGHBOUR BOLTING {:.2}", i.hunter, i.herd), dim));
        }
        if let Some(k) = ent.get::<&Kin>().map(|k| k.group) {
            let n = self.ents.world.query::<&Kin>().iter().filter(|q| q.group == k).count();
            lines.push((format!("HERD {k}: {n} MEMBERS"), dim));
        } else {
            lines.push(("NO HERD".to_string(), dim));
        }
        if let Some(show) = ent.get::<&crate::creatures::display::Show>() {
            lines.push((format!("DISPLAY {}: {}", show.kind, show.phase).to_uppercase(), dim));
        }
        if let Some(t) = insp.as_deref().and_then(|i| i.target.clone()) {
            lines.push((format!("TARGET {t}").to_uppercase(), dim));
        }
        if let Some(i) = insp.as_deref() {
            lines.push((String::new(), dim));
            lines.push(("LAST DECISIONS".to_string(), white));
            let now = self.ents.clock;
            for (t, l) in i.log.iter().rev() {
                lines.push((format!("{:>5.1} S AGO  {}", now - t, l.to_uppercase()), quiet));
            }
        }
        drop((crit, insp, life));
        let ls = s * if at.is_some() { 0.75 } else { 0.85 };
        let wmax = lines.iter().map(|l| Hud::text_width(&l.0, ls)).fold(0.0, f32::max);
        let lh = line * ls / s * 1.05;
        let (x, y0) = at.unwrap_or(((w - wmax - 24.0).floor(), (h * 0.12).floor()));
        let rows = lines.len() as f32;
        hud.rect(x - 10.0, y0 - 8.0, wmax + 20.0, rows * lh + 14.0, [0.0, 0.0, 0.0, 0.6]);
        for (i, (l, c)) in lines.iter().enumerate() {
            hud.label(x, y0 + i as f32 * lh, ls, *c, l);
        }
    }

    /// The pinned animal and its herd drawn into the world (D3): headings,
    /// the probes it cast (blocked in red), lines to its kin, its flee
    /// radius on the ground, and the player's scent cone down the wind.
    pub(super) fn eco_overlay(&mut self) {
        let Some(e) = self.eco.pinned else { return };
        let alpha = self.alpha;
        let world = &self.ents.world;
        let Ok(ent) = world.entity(e) else { return };
        let Some(lk) = ent.get::<&Look>().map(|l| l.prev_pos.lerp(l.pos, alpha)) else { return };
        let tall = ent.get::<&crate::physics::Body>().map_or(1.0, |b| b.height);
        let group = ent.get::<&Kin>().map(|k| k.group);
        let flee = ent.get::<&Inspect>().map_or(0.0, |i| i.flee_m);
        let probes = ent.get::<&Inspect>().map(|i| i.probes.clone()).unwrap_or_default();
        let vel = if self.player.fly { Vec3::ZERO } else { self.player.body.vel };
        let threat = self.player_threat(vel);
        let gen = self.streamer.generator();
        let ground = |x: f32, z: f32| gen.height(x, z).max(self.sea) + 0.25;
        let ov = &mut self.overlay;
        let yellow = [1.0, 0.9, 0.2, 0.95];
        let mid = lk + Vec3::Y * tall * 0.5;
        // Probes from the feet, as walk.rs casts them.
        for (a, reach, ok) in probes {
            let c = if ok { [0.3, 1.0, 0.4, 0.8] } else { [1.0, 0.2, 0.15, 0.95] };
            let from = lk + Vec3::Y * 0.3;
            ov.line_on_top(from, from + heading_dir(a) * reach, 0.03, c);
        }
        // Headings for it and its herd, and lines to each of its kin.
        for (oe, look, crit, kin, body) in world.query::<(hecs::Entity, &Look, &Critter, Option<&Kin>, &crate::physics::Body)>().iter() {
            let mine = oe == e;
            if !mine && (group.is_none() || kin.map(|k| k.group) != group) {
                continue;
            }
            let at = look.prev_pos.lerp(look.pos, alpha) + Vec3::Y * body.height * 0.5;
            let len = if mine { 2.0 + body.height } else { 1.0 + body.height * 0.6 };
            ov.line_on_top(at, at + heading_dir(crit.heading) * len, if mine { 0.06 } else { 0.035 }, yellow);
            if !mine {
                ov.line_on_top(mid, at, 0.025, [0.4, 0.85, 1.0, 0.7]);
            }
        }
        // The flee radius round it, on the ground.
        let ring = |ov: &mut crate::render::overlay::Overlay, c: Vec3, r: f32, col: [f32; 4]| {
            let n = ((r * 1.5) as usize).clamp(24, 96);
            let pt = |i: usize| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                let (x, z) = (c.x + a.cos() * r, c.z + a.sin() * r);
                Vec3::new(x, ground(x, z), z)
            };
            for i in 0..n {
                ov.line_on_top(pt(i), pt(i + 1), 0.06, col);
            }
        };
        if flee > 0.0 {
            ring(ov, lk, flee, [1.0, 0.55, 0.2, 0.7]);
        }
        // The player's scent: a cone down the wind as far as it reaches this
        // animal (startle.rs `scent_at`, `SCENT_REACH`), or a pool round them
        // on a still day.
        let pp = self.player.pos;
        let carry = threat.scent.length();
        let purple = [0.8, 0.45, 1.0, 0.7];
        if carry > 0.0 && flee > 0.0 {
            let reach = flee * (1.0 + 1.5 * carry.min(1.0));
            let down = threat.scent / carry;
            let base = down.x.atan2(down.z);
            let edge = 0.7f32.acos();
            let n = 16;
            let pt = |a: f32| {
                let q = pp + heading_dir(a) * reach;
                Vec3::new(q.x, ground(q.x, q.z), q.z)
            };
            let foot = Vec3::new(pp.x, ground(pp.x, pp.z), pp.z);
            ov.line_on_top(foot, pt(base - edge), 0.06, purple);
            ov.line_on_top(foot, pt(base + edge), 0.06, purple);
            for i in 0..n {
                let a = base - edge + 2.0 * edge * i as f32 / n as f32;
                let b = base - edge + 2.0 * edge * (i + 1) as f32 / n as f32;
                ov.line_on_top(pt(a), pt(b), 0.06, purple);
            }
        } else if threat.pool > 0.0 && flee > 0.0 {
            ring(ov, pp, flee * (1.0 + 1.5 * threat.pool * 0.5), purple);
        }
    }

    /// Every animal now, for `--eco-dump` (D5): one JSON object a line.
    pub fn eco_snapshot(&self) -> serde_json::Value {
        use serde_json::json;
        let world = &self.ents.world;
        let mut counts = serde_json::Map::new();
        let mut tally = [0usize; STATES.len()];
        let mut herds = crate::util::FxHashSet::default();
        let mut list = Vec::new();
        for (e, look, c, crit, show, distant, kin, age, sex, anim) in world
            .query::<(
                hecs::Entity,
                &Look,
                &Creature,
                Option<&Critter>,
                Option<&crate::creatures::display::Show>,
                Option<&spawn::Distant>,
                Option<&Kin>,
                Option<&crate::creatures::age::Age>,
                Option<&crate::creatures::sex::Sex>,
                Option<&crate::anim::Anim>,
            )>()
            .iter()
        {
            let st = State::of(crit, show.is_some());
            tally[st as usize] += 1;
            if let Some(k) = kin {
                herds.insert(k.group);
            }
            let r1 = |v: f32| (v as f64 * 10.0).round() / 10.0;
            let mut o = json!({
                "id": e.id(),
                "species": self.ents.species[c.species].name,
                "pos": [r1(look.pos.x), r1(look.pos.y), r1(look.pos.z)],
                "state": st.name(),
            });
            let m = o.as_object_mut().unwrap();
            if let Some(c) = crit {
                m.insert("mood".into(), json!(c.mood.name()));
                if c.herded && c.mood == Mood::Flee {
                    m.insert("herded".into(), json!(true));
                }
                m.insert("alarm".into(), json!((c.alarm as f64 * 100.0).round() / 100.0));
            }
            if let Some(a) = anim.and_then(|a| a.life.as_ref()) {
                m.insert("act".into(), json!(a.act.name()));
            }
            if let Some(k) = kin {
                m.insert("herd".into(), json!(k.group));
            }
            if let Some(a) = age {
                m.insert("stage".into(), json!(a.stage.name()));
            }
            if let Some(s) = sex {
                m.insert("sex".into(), json!(s.name()));
            }
            if let Some(s) = show {
                m.insert("display".into(), json!(format!("{}: {}", s.kind, s.phase)));
            }
            if distant.is_some() {
                m.insert("distant".into(), json!(true));
            }
            list.push(o);
        }
        for st in STATES {
            counts.insert(st.name().into(), json!(tally[st as usize]));
        }
        let p = self.player.pos;
        let mut out = json!({
            "t": (self.ents.clock as f64 * 10.0).round() / 10.0,
            "hour": (self.hour as f64 * 100.0).round() / 100.0,
            "player": [p.x.round(), p.y.round(), p.z.round()],
            "counts": counts,
            "herds": herds.len(),
            "creatures": list,
        });
        let trips: Vec<serde_json::Value> = self
            .ents
            .roam
            .trips()
            .map(|(g, c, n, t)| {
                let goal = t.route.last().copied().unwrap_or(c);
                json!({
                    "herd": g,
                    "members": n,
                    "at": [c.x.round(), c.z.round()],
                    "need": t.need.name(),
                    "goal": [goal.x.round(), goal.z.round()],
                    "to_go": ((goal - c) * Vec3::new(1.0, 0.0, 1.0)).length().round(),
                    "arrived": t.arrived.is_some(),
                    "quarry": t.quarry,
                })
            })
            .collect();
        out["trips"] = json!(trips);
        let r = &self.ents.roster;
        let records: Vec<serde_json::Value> = r
            .herds
            .iter()
            .map(|h| {
                let stages: Vec<&str> = h.members.iter().map(|m| m.stage.name()).collect();
                json!({
                    "id": h.id,
                    "species": self.ents.species[h.species].name,
                    "kind": crate::creatures::roster::kind(&self.ents.species[h.species]),
                    "wary": (self.ents.roam.wary_of(h.id) as f64 * 100.0).round() / 100.0,
                    "size": h.size,
                    "at": [h.centre.x.round(), h.centre.z.round()],
                    "drawn": h.drawn,
                    "kept": stages,
                })
            })
            .collect();
        let kills: Vec<serde_json::Value> = r
            .kills
            .iter()
            .map(|k| {
                json!({
                    "at": [k.at.x.round(), k.at.z.round()],
                    "victim": self.ents.species[k.victim.species].name,
                    "by": self.ents.species[k.by].name,
                    "when": k.when.round(),
                    "down": k.ent.is_some(),
                })
            })
            .collect();
        out["roster"] = json!({"herds": r.herds.len(), "drawn": r.drawn, "stored": r.stored, "lost": r.lost, "killed": r.killed, "kills": kills, "list": records});
        if !self.ents.live.is_empty() {
            let r2 = |v: f32| (v as f64 * 100.0).round() / 100.0;
            let stats: serde_json::Map<String, serde_json::Value> =
                self.ents.live.stats().into_iter().map(|(n, sum, max)| (n.to_string(), json!({"sum": r2(sum), "max": r2(max)}))).collect();
            out["live"] = json!({"ticks": self.ents.live.ticks, "tick_ms": r2(self.ents.live.tick_ms), "chans": stats});
        }
        if let Some(e) = self.eco.pinned {
            if let Ok(i) = world.get::<&Inspect>(e) {
                let log: Vec<_> = i.log.iter().map(|(t, l)| json!([(*t as f64 * 10.0).round() / 10.0, l])).collect();
                let s = i.player;
                let r2 = |v: f32| (v as f64 * 100.0).round() / 100.0;
                out["pinned"] = json!({
                    "id": e.id(),
                    "senses": {"sight": r2(s.sight), "noise": r2(s.noise), "scent": r2(s.scent), "close": r2(s.close)},
                    "hunter": r2(i.hunter),
                    "herd": r2(i.herd),
                    "flee_m": r2(i.flee_m),
                    "log": log,
                });
            }
        }
        out
    }
}
