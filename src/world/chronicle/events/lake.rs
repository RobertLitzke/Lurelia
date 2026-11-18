use crate::util::FxHashMap as HashMap;
use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Pos, Wet};
use crate::world::noise::perlin2;
use glam::Vec2;
use serde::Deserialize;

/// A lake: a basin dug at `at`, about `radius` m across its water, filled to
/// a hand below the lowest ground round its shore. Still water: the same
/// surface, banks and swimming as a river, with no current. A river whose
/// path ends in it runs into it at its level.
#[derive(Deserialize, Debug)]
pub struct Lake {
    name: String,
    at: Pos,
    radius: f32,
    /// Water depth in the middle, metres.
    depth: f32,
    /// The lake bed under the water.
    bed: String,
    /// Mud or sand along the waterline: the shallow edge of the bed and a
    /// ragged strip of shore. Without it the bed runs to the edge.
    #[serde(default)]
    bank: Option<String>,
    /// How far the shore wanders in and out, as a share of the radius
    /// (0 a circle; default 0.3, coves and points).
    #[serde(default = "default_ragged")]
    ragged: f32,
    /// Stretch: the lake is this many times longer than wide, along
    /// `bearing` (compass degrees). Default 1, round.
    #[serde(default = "one")]
    stretch: f32,
    #[serde(default)]
    bearing: f32,
    /// Leave the ground round the shore as it is, instead of grading it
    /// back to a gentle bank: a plunge pool under a cliff, a tarn in a
    /// rock bowl.
    #[serde(default)]
    steep: bool,
}

fn default_ragged() -> f32 {
    0.3
}

fn one() -> f32 {
    1.0
}

#[derive(Clone)]
pub struct LakeRec {
    pub centre: Vec2,
    pub radius: f32,
    pub depth: f32,
    pub bed: u8,
    pub bank: u8,
    pub ragged: f32,
    pub stretch: f32,
    /// Unit vector of the long axis (x, z).
    pub axis: Vec2,
    pub seed: u32,
    pub steep: bool,
}

/// Every lake so far, by name.
#[derive(Default)]
pub struct Lakes(pub HashMap<String, LakeRec>);

impl Process for Lake {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let centre = cx.pos(&self.at);
        let bed = cx.mat(&self.bed);
        let bank = self.bank.as_ref().map_or(0, |b| cx.mat(b));
        let b = self.bearing.to_radians();
        let rec = LakeRec {
            centre,
            radius: self.radius.max(3.0),
            depth: self.depth.max(0.5),
            bed,
            bank,
            ragged: self.ragged.clamp(0.0, 0.8),
            stretch: self.stretch.max(1.0),
            // Compass bearing: 0 north (-z), 90 east (+x).
            axis: Vec2::new(b.sin(), -b.cos()),
            seed: cx.seed,
            steep: self.steep,
        };
        // Dug now, so later events (a village on its shore, a river into it)
        // see the basin; shaped again, with its water, after every event.
        let mut none = HashMap::default();
        let level = shape(&mut cx.f, &rec, &mut none, false);
        cx.state.get::<Lakes>().0.insert(self.name.clone(), rec);
        format!("{}: {:.0} m across, water at {:.1} m", self.name, self.radius * 2.0, level)
    }
}

/// Distance from the lake's middle to `p` in units of its shore distance
/// that way (below 1 is water), and metres out past the shore.
fn reach(rec: &LakeRec, p: Vec2) -> (f32, f32) {
    let d = p - rec.centre;
    let along = d.dot(rec.axis);
    let across = d.perp_dot(rec.axis);
    // Distance in the unstretched circle.
    let q = Vec2::new(along / rec.stretch, across);
    let r = q.length();
    let a = q.y.atan2(q.x);
    // The shore wanders in coves and points, smoothly round the lake.
    let wobble = perlin2(a.cos() * 1.3 + 7.1, a.sin() * 1.3, rec.seed) * 0.8 + perlin2(a.cos() * 3.1, a.sin() * 3.1 + 2.3, rec.seed ^ 0x9e) * 0.35;
    let shore = rec.radius * (1.0 + rec.ragged * wobble).max(0.3);
    // Metres past the shore, measured in the stretched (real) space roughly.
    let scale = if r > 1e-3 { d.length() / r } else { 1.0 };
    (r / shore, (r - shore) * scale)
}

/// Dig the basin and, with `water`, fill it: `f.inland` gets the lake's
/// cells and a margin of shore, each claimed in `owner` (lakes win over
/// rivers). Returns the water level.
pub fn shape(f: &mut Fields, rec: &LakeRec, owner: &mut HashMap<u32, f32>, water: bool) -> f32 {
    let outer = rec.radius * rec.stretch * (1.0 + rec.ragged * 1.2) + 8.0;
    // The lowest ground just round the shore sets the water level (not a
    // river channel running in or out).
    let mut rim = f32::MAX;
    each_near(f, rec.centre, outer, |f: &mut Fields, i, _, p| {
        let (s, _) = reach(rec, p);
        if (1.15..1.45).contains(&s) && !f.occupied[i] && f.water[i] != 1 {
            rim = rim.min(f.h[i]);
        }
    });
    if rim == f32::MAX {
        rim = f.height(rec.centre.x, rec.centre.y);
    }
    let level = (rim - 0.3).max(f.sea);
    let sea = f.sea;
    let seed = rec.seed;
    let mut cells: Vec<usize> = Vec::new();
    each_near(f, rec.centre, outer, |_f: &mut Fields, i, _, _| cells.push(i));
    for i in cells {
        let p = f.centre(i);
        let (s, out) = reach(rec, p);
        let occupied = f.occupied[i];
        if s < 1.0 {
            // A bowl, shelving gently near the shore.
            f.h[i] = level - 0.05 - (rec.depth - 0.05) * (1.0 - s * s).max(0.0).powf(0.6);
            f.water[i] = 1;
            let edge = 0.8 + 0.1 * perlin2(p.x * 0.3, p.y * 0.3, seed ^ 0x77);
            f.surface[i] = if rec.bank != 0 && s > edge { rec.bank } else { rec.bed };
        } else {
            let bank = level - 0.05 + out * 0.4;
            // A river channel running out of the lake keeps its bed (no lip
            // across it); a steep shore keeps its rock past the waterline.
            let channel = f.water[i] == 1;
            if !occupied && !channel {
                if f.h[i] > bank && (!rec.steep || out < 1.0) {
                    f.h[i] = bank;
                } else if f.h[i] <= bank && out < super::river::MARGIN && level > sea + 0.02 {
                    // A low lip where the land falls away below the water.
                    f.h[i] = f.h[i].max(bank.min(level + 0.15));
                }
            }
            if f.water[i] == 1 && water {
                f.water[i] = 0;
            }
            let fringe = 0.3 + 1.2 * (perlin2(p.x * 0.15, p.y * 0.15, seed ^ 0x33) * 0.5 + 0.5);
            if rec.bank != 0 && out < fringe && !occupied {
                f.surface[i] = rec.bank;
            }
        }
        if water && out < super::river::MARGIN && level > sea + 0.02 {
            owner.insert(i as u32, f32::NEG_INFINITY);
            f.inland.insert(i as u32, Wet { level, flow: [0.0, 0.0] });
        }
    }
    level
}
