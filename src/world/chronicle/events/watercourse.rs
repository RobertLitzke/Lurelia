use super::river::{carve, RiverRec, Rivers};
use crate::world::chronicle::geom::densify;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use crate::world::route::{self, cost};
use glam::Vec2;
use serde::Deserialize;

/// A channel that finds its own way down, and the distributaries that leave
/// it. Where `river` takes the course as a list of points someone drew, this
/// searches for one: only the rise of a step is paid for, falling is free,
/// and a step that climbs more than a few tens of centimetres is refused, so
/// the line that comes back is one water could take. On a delta that is the
/// difference between channels drawn across the land and channels that found
/// the low ground.
///
/// Each branch leaves the trunk a fraction of the way along it and goes to
/// its own mouth. Branches pay to run over ground the trunk already took, so
/// they diverge instead of falling straight back into it.
#[derive(Deserialize, Debug)]
pub struct Watercourse {
    name: String,
    from: Pos,
    to: Pos,
    /// Points the channel must pass through on its way down. The line between
    /// each pair is found, not drawn, so a river keeps the geography a world
    /// is built round (the bend a village stands on, the gap it comes through)
    /// while still running where water would.
    #[serde(default)]
    via: Vec<Pos>,
    width: f32,
    depth: f32,
    /// The river bed under the water.
    bed: String,
    /// Mud or sand along the waterline.
    #[serde(default)]
    bank: Option<String>,
    /// How far the channel wanders off the line it found, metres.
    #[serde(default = "default_meander")]
    meander: f32,
    /// The deepest the bed may be cut under the sea.
    #[serde(default)]
    floor: Option<f32>,
    /// Water this deep in the trunk's middle, metres, instead of the
    /// swimmable 1.4 m or more every river gets: a wide knee-deep riffle
    /// over cobbles where salmon run (Bonebight). Give it a shallow `depth`
    /// too, or the water stands low in a deep cut.
    #[serde(default)]
    shallow: Option<f32>,
    #[serde(default)]
    branches: Vec<Branch>,
    #[serde(default)]
    terms: route::Terms,
    /// Metres over which it grows from a trickle to its full size (see
    /// `river`). 0: full-grown from its first metre.
    #[serde(default)]
    rise: f32,
    /// It comes in from beyond the world: carried back to the nearest edge.
    #[serde(default)]
    from_edge: bool,
    /// Patches of a second bed (eelgrass in a slow stretch) over part of
    /// the trunk.
    #[serde(default)]
    weed: Option<Weed>,
}

/// Patches of `mat` over about `share` of the bed, from the trunk's head to
/// `until` of the way to its mouth (0 to 1).
#[derive(Deserialize, Debug)]
struct Weed {
    mat: String,
    #[serde(default = "default_weed_share")]
    share: f32,
    #[serde(default = "default_weed_until")]
    until: f32,
}

fn default_weed_share() -> f32 {
    0.4
}

fn default_weed_until() -> f32 {
    1.0
}

#[derive(Deserialize, Debug)]
struct Branch {
    name: String,
    to: Pos,
    /// How far along the trunk it leaves, 0 at the head and 1 at the mouth.
    #[serde(default)]
    at: f32,
    /// The trunk's own, scaled by `taper`, when left out.
    #[serde(default)]
    width: Option<f32>,
    #[serde(default)]
    depth: Option<f32>,
    #[serde(default)]
    meander: Option<f32>,
}

fn default_meander() -> f32 {
    14.0
}

/// How much narrower and shallower a branch is than the trunk it left.
const TAPER: f32 = 0.62;

impl Process for Watercourse {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let (a, b) = (cx.pos(&self.from), cx.pos(&self.to));
        let mut terms = self.terms.over_water();
        if terms.step > 6.0 {
            terms.step = 6.0;
        }
        let mut grid = cost::Grid::new(&cx.f, terms.step);
        let mut stops = vec![a];
        for v in &self.via {
            let p = cx.pos(v);
            stops.push(p);
        }
        stops.push(b);
        let mut trunk: Vec<Vec2> = Vec::new();
        for w in stops.windows(2) {
            let Some(leg) = route::find_line(&grid, w[0], w[1], &terms) else {
                cx.warnings.push(format!("watercourse {}: no way down from {:.0},{:.0} to {:.0},{:.0}", self.name, w[0].x, w[0].y, w[1].x, w[1].y));
                return "skipped: no way down".into();
            };
            if trunk.is_empty() {
                trunk = leg;
            } else {
                trunk.extend(leg.into_iter().skip(1));
            }
        }
        let bed = cx.mat(&self.bed);
        let bank = self.bank.as_ref().map_or(0, |x| cx.mat(x));
        let trunk = wander(&trunk, self.meander, cx.seed);
        let trunk = if self.from_edge { super::river::to_edge(&cx.f, &trunk) } else { trunk };
        let fall = cx.f.height(a.x, a.y) - cx.f.height(b.x, b.y);
        let grown = super::river::head(cx, &self.name, &trunk, self.width, self.depth, bed, bank, self.floor, self.rise);
        cut(cx, &self.name, &trunk[grown..], self.width, self.depth, bed, bank, self.floor);
        if let Some(d) = self.shallow {
            if let Some(r) = cx.state.get::<Rivers>().0.get_mut(&self.name) {
                r.shallow = Some(d);
            }
        }
        let weeded = self.weed.as_ref().map_or(0, |w| weed(cx, w, &trunk, self.width, bed));
        // The trunk's ground is dear to a branch: a distributary that fell
        // straight back into it would not be one.
        grid.take(&trunk, self.width * 0.5 + 2.0);
        let mut cut_branches = 0;
        let mut names = Vec::new();
        for (k, br) in self.branches.iter().enumerate() {
            let head = trunk[((br.at.clamp(0.0, 1.0) * (trunk.len() - 1) as f32) as usize).min(trunk.len() - 1)];
            let mouth = cx.pos(&br.to);
            let mut bt = terms;
            bt.taken = 3.0;
            let Some(line) = route::find_line(&grid, head, mouth, &bt) else {
                cx.warnings.push(format!("watercourse {}: the {} branch found no way down", self.name, br.name));
                continue;
            };
            let line = wander(&line, br.meander.unwrap_or(self.meander * 0.8), cx.seed ^ (k as u32 + 1).wrapping_mul(0x9e37_79b9));
            let w = br.width.unwrap_or(self.width * TAPER);
            let dpt = br.depth.unwrap_or(self.depth * TAPER);
            cut(cx, &br.name, &line, w, dpt, bed, bank, self.floor);
            grid.take(&line, w * 0.5 + 2.0);
            cut_branches += 1;
            names.push(br.name.clone());
        }
        let len: f32 = trunk.windows(2).map(|w| w[0].distance(w[1])).sum();
        format!(
            "{}: {len:.0} m found down a fall of {fall:.1} m, {width} m wide{}{}",
            self.name,
            if cut_branches == 0 {
                String::new()
            } else {
                format!("; {cut_branches} distributaries ({})", names.join(", "))
            },
            if weeded > 0 { format!("; {weeded} m2 of {}", self.weed.as_ref().map_or("", |w| w.mat.as_str())) } else { String::new() },
            width = self.width,
        )
    }
}

/// Lay the patches of a second bed over the trunk's bed from its head to
/// `until` of its length, in noise blobs a few metres across; the cells
/// changed.
fn weed(cx: &mut EventCtx, w: &Weed, trunk: &[Vec2], width: f32, bed: u8) -> usize {
    let mat = cx.mat(&w.mat);
    let total: f32 = trunk.windows(2).map(|p| p[0].distance(p[1])).sum();
    let stop = total * w.until.clamp(0.0, 1.0);
    let cut = 1.0 - 2.0 * w.share.clamp(0.0, 1.0);
    let seed = cx.seed ^ 0x3ee1;
    let r = (width * 0.5 + 1.0).ceil() as i32;
    let f = &mut cx.f;
    let mut n = 0;
    let mut run = 0.0;
    for p in trunk.windows(2) {
        if run > stop {
            break;
        }
        run += p[0].distance(p[1]);
        for dz in -r..=r {
            for dx in -r..=r {
                let (x, z) = (p[0].x + dx as f32, p[0].y + dz as f32);
                if x < 0.0 || z < 0.0 || x >= f.size || z >= f.size {
                    continue;
                }
                let i = f.idx(x, z);
                if f.surface[i] == bed && crate::world::noise::perlin2(x * 0.15, z * 0.15, seed) > cut {
                    f.surface[i] = mat;
                    n += 1;
                }
            }
        }
    }
    n
}

/// Carve a channel and register it, so the water settles into it after the
/// chronicle like any other river's.
///
/// The last quarter is cut in two shallower, narrower steps. A distributary
/// does not stop: it runs out, and a channel carved to full depth to its last
/// metre leaves a blunt trench end sitting in open water.
fn cut(cx: &mut EventCtx, name: &str, line: &[Vec2], width: f32, depth: f32, bed: u8, bank: u8, floor: Option<f32>) {
    // A channel stops once it reaches water already deeper than itself:
    // carrying it on only digs a trench across a lagoon floor.
    let deep = cx.f.sea - depth * 0.8;
    let keep = line.iter().rposition(|p| cx.f.height(p.x, p.y) > deep).map_or(line.len(), |k| (k + 3).min(line.len()));
    let line = &line[..keep.max(2.min(line.len()))];
    let n = line.len();
    let at = |f: f32| ((n - 1) as f32 * f) as usize;
    carve(cx, &line[..=at(0.78)], width, depth, bed, floor);
    if n > 12 {
        carve(cx, &line[at(0.75)..=at(0.92)], width * 0.72, depth * 0.6, bed, floor);
        carve(cx, &line[at(0.90)..], width * 0.48, depth * 0.3, bed, floor);
    } else {
        carve(cx, line, width, depth, bed, floor);
    }
    let seed = cx.seed;
    cx.state.get::<Rivers>().0.insert(
        name.to_string(),
        RiverRec {
            line: line.to_vec(),
            width,
            depth,
            bed,
            bank,
            seed,
            floor,
            shallow: None,
        },
    );
}

/// A found line is straighter than water is: give it back its meanders.
fn wander(line: &[Vec2], meander: f32, seed: u32) -> Vec<Vec2> {
    if meander <= 0.0 {
        return line.to_vec();
    }
    densify(line, meander, seed)
}
