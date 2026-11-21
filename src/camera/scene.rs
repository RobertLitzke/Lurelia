//! What a photo shows, measured at the moment the shutter fires: every
//! creature and tree near enough to matter, where it sits in the frame, how
//! big, how sharp, whether something hides it, what is behind it, and what it
//! was doing. Tasks (tasks.rs) are judged from this alone, so the checks can
//! be tested without a world.

use super::lens::{Lens, Orient};
use crate::creatures::circle::Gull;
use crate::creatures::flit::Flit;
use crate::creatures::flock::Flyer;
use crate::creatures::locomotion::Mover;
use crate::creatures::walk::Critter;
use crate::creatures::{Creature, Look, Species};
use crate::models::Model;
use crate::world::gen::Generator;
use crate::world::store::Store;
use glam::Vec3;
use serde::{Deserialize, Serialize};

/// What an animal was doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Doing {
    Eating,
    Drinking,
    Resting,
    Moving,
    Standing,
    Flying,
    Perched,
}

impl Doing {
    pub fn phrase(self) -> &'static str {
        match self {
            Doing::Eating => "EATING",
            Doing::Drinking => "DRINKING",
            Doing::Resting => "RESTING",
            Doing::Moving => "ON THE MOVE",
            Doing::Standing => "STANDING STILL",
            Doing::Flying => "IN FLIGHT",
            Doing::Perched => "PERCHED",
        }
    }
}

/// One creature or tree as the photo saw it.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// Upper case, spaces for underscores ("RED FOX").
    pub name: String,
    /// What it is a kind of (`like` in the library), so a silver fox counts as a fox.
    pub like: Option<String>,
    pub doing: Option<Doing>,
    /// Centre in the frame: 0..1 across and down. Outside 0..1 is out of frame.
    pub u: f32,
    pub v: f32,
    /// Height as a fraction of the frame's height.
    pub size: f32,
    pub dist: f32,
    /// Degrees above (positive) or below the camera.
    pub elev: f32,
    /// Not hidden behind ground, rock or leaves.
    pub visible: bool,
    pub sharp: bool,
    /// Open sky behind it (nothing within 300 m along the line of sight).
    pub sky_behind: bool,
    /// Blur of what is behind it, pixels at 1080 tall.
    pub bg_blur: f32,
}

impl Seen {
    pub fn in_frame(&self) -> bool {
        (0.0..=1.0).contains(&self.u) && (0.0..=1.0).contains(&self.v)
    }

    /// Is it a `name` (or a kind of one)?
    pub fn is(&self, name: &str) -> bool {
        self.name == name || self.like.as_deref() == Some(name)
    }
}

/// Everything the tasks look at.
#[derive(Clone, Debug, Default)]
pub struct Shot {
    pub hour: f32,
    pub orient: Orient,
    pub lens: Lens,
    /// Creatures and trees in or near the frame.
    pub seen: Vec<Seen>,
    /// Some water (sea, lake or river) shows in the frame.
    pub water: bool,
}

/// The camera at the moment of the photo.
pub struct View {
    pub eye: Vec3,
    pub fwd: Vec3,
    pub up: Vec3,
    /// Vertical field of view, radians.
    pub fov: f32,
    /// Screen width over height.
    pub aspect: f32,
}

impl View {
    fn right(&self) -> Vec3 {
        self.fwd.cross(self.up).normalize()
    }

    /// Where `p` falls in the frame (u, v in 0..1 inside it), or None behind the camera.
    fn project(&self, p: Vec3, frame: [f32; 4]) -> Option<(f32, f32)> {
        let d = p - self.eye;
        let z = d.dot(self.fwd);
        if z < 0.05 {
            return None;
        }
        let t = (self.fov * 0.5).tan();
        let sx = d.dot(self.right()) / (z * t * self.aspect);
        let sy = d.dot(self.up) / (z * t);
        let (su, sv) = (sx * 0.5 + 0.5, 0.5 - sy * 0.5);
        Some(((su - frame[0]) / (frame[2] - frame[0]), (sv - frame[1]) / (frame[3] - frame[1])))
    }

    /// A ray through (u, v) of the frame.
    fn ray(&self, u: f32, v: f32, frame: [f32; 4]) -> Vec3 {
        let su = frame[0] + u * (frame[2] - frame[0]);
        let sv = frame[1] + v * (frame[3] - frame[1]);
        let t = (self.fov * 0.5).tan();
        (self.fwd + self.right() * ((su * 2.0 - 1.0) * t * self.aspect) + self.up * ((1.0 - sv * 2.0) * t)).normalize()
    }
}

/// What the camera sees from `view`, through `lens`, framed by `orient`.
#[allow(clippy::too_many_arguments)]
pub fn look(view: &View, lens: Lens, orient: Orient, hour: f32, world: &hecs::World, species: &[Species], models: &[Model], store: &Store, gen: &Generator, sea: f32) -> Shot {
    let frame = super::lens::frame_uv(orient, view.aspect);
    let frame_h = (frame[3] - frame[1]) * view.fov; // radians of frame height, near enough
    let mut seen = Vec::new();
    const RANGE: f32 = 120.0;
    let mut add = |name: String, like: Option<String>, doing: Option<Doing>, base: Vec3, height: f32, hit_ok: &dyn Fn(Vec3) -> bool| {
        let centre = base + Vec3::Y * height * 0.5;
        let to = centre - view.eye;
        let dist = to.length();
        if !(0.3..RANGE).contains(&dist) {
            return;
        }
        let Some((u, v)) = view.project(centre, frame) else {
            return;
        };
        // Only bother with things near the frame.
        if !(-0.5..=1.5).contains(&u) || !(-0.5..=1.5).contains(&v) {
            return;
        }
        // Visible if a line to its middle or to its upper part is clear.
        let clear = |p: Vec3| {
            let d = p - view.eye;
            let l = d.length();
            match store.ray(view.eye, d, l, |m| m != crate::world::material::AIR) {
                None => true,
                Some(h) => h.t >= l - 0.35 || hit_ok(voxel_centre(h.voxel)),
            }
        };
        let visible = clear(centre) || clear(base + Vec3::Y * height * 0.85);
        // Behind it, along the line of sight.
        let dir = to / dist;
        let start = centre + dir * (height * 0.6 + 0.5);
        let behind = store.ray(start, dir, 300.0, |m| m != crate::world::material::AIR).map(|h| h.t + dist);
        let sea_t = if dir.y < -1e-4 && start.y > sea { Some((start.y - sea) / -dir.y + dist) } else { None };
        let bg = match (behind, sea_t) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let sky_behind = bg.is_none() && dir.y > -0.02;
        let bg_blur = lens.blur(bg.unwrap_or(f32::INFINITY).min(1e6));
        seen.push(Seen {
            name,
            like,
            doing,
            u,
            v,
            size: (height / dist) / frame_h.max(1e-3),
            dist,
            elev: (to.y / dist).asin().to_degrees(),
            visible,
            sharp: lens.sharp(dist),
            sky_behind,
            bg_blur,
        });
    };

    let never = |_: Vec3| false;
    for (look, me, crit, gull, anim, flit, flyer, morph) in world
        .query::<(
            &Look,
            Option<&Creature>,
            Option<&Critter>,
            Option<&Gull>,
            Option<&crate::anim::Anim>,
            Option<&Flit>,
            Option<&Flyer>,
            Option<&crate::variety::Morph>,
        )>()
        .iter()
    {
        let (name, like) = match (me, crit, gull) {
            (Some(c), _, _) => match species.get(c.species) {
                Some(sp) => {
                    // Fish and whales are out of reach under the water.
                    if matches!(sp.verb().mover, Mover::School | Mover::Breach | Mover::Creep) && look.pos.y < sea - 0.3 {
                        continue;
                    }
                    (display(&sp.name), sp.like.as_deref().map(display))
                }
                None => continue,
            },
            (_, Some(_), _) => (crate::creatures::spawn::legacy(look.model).map_or("ANIMAL", |l| l.name).to_uppercase(), None),
            (_, _, Some(_)) => ("GULL".to_string(), None),
            _ => continue,
        };
        let m = &models[look.model];
        let height = (m.height * morph.map_or(1.0, |m| m.scale)).max(0.15);
        let doing = doing_of(look, anim, flit, flyer);
        add(name, like, Some(doing), look.pos, height, &never);
    }

    let trees = gen.trees_near(view.eye.x - RANGE, view.eye.z - RANGE, view.eye.x + RANGE, view.eye.z + RANGE);
    // Herbs, small shrubs and underwater plants (kelp) are left out.
    for t in trees.iter().filter(|t| t.top >= 1.5 && t.pos.y > sea - 0.2) {
        let name = tree_name(gen, t);
        // A ray that stops in this tree's own crown or trunk still sees it.
        let (pos, r, top) = (t.pos, t.radius.max(1.0) + 0.75, t.top);
        let own = move |p: Vec3| Vec3::new(p.x - pos.x, 0.0, p.z - pos.z).length() < r && p.y > pos.y - 0.5 && p.y < pos.y + top + 1.0;
        add(name, None, None, t.pos, t.top.max(1.0), &own);
    }

    Shot {
        hour,
        orient,
        lens,
        seen,
        water: water_in_frame(view, frame, store, sea),
    }
}

fn voxel_centre(v: [i32; 3]) -> Vec3 {
    (Vec3::new(v[0] as f32, v[1] as f32, v[2] as f32) + 0.5) * 0.25
}

/// "red_fox" -> "RED FOX".
pub fn display(name: &str) -> String {
    name.replace('_', " ").to_uppercase()
}

/// A tree's name: its library plant, or its kind on the original island.
pub fn tree_name(gen: &Generator, t: &crate::world::gen::Tree) -> String {
    match gen.world.as_ref().and_then(|w| w.lib.plants.get(t.plant as usize)) {
        Some(pl) => display(&pl.name),
        None => match t.kind {
            crate::world::gen::TreeKind::Broad => "BROADLEAF".to_string(),
            crate::world::gen::TreeKind::Birch => "BIRCH".to_string(),
            crate::world::gen::TreeKind::Pine => "PINE".to_string(),
            crate::world::gen::TreeKind::Vine => "VINE".to_string(),
            crate::world::gen::TreeKind::Crust => "MOSS".to_string(),
            crate::world::gen::TreeKind::Bracket => "FUNGUS".to_string(),
        },
    }
}

fn doing_of(look: &Look, anim: Option<&crate::anim::Anim>, flit: Option<&Flit>, flyer: Option<&Flyer>) -> Doing {
    use crate::anim::life::Act;
    let speed = (look.pos - look.prev_pos).length() / crate::game::STEP;
    if let Some(l) = anim.and_then(|a| a.life.as_ref()) {
        // Anything actually covering ground is on the move, whatever it
        // planned to be doing: a grazer that has been startled is running,
        // and the photo shows that rather than the plan.
        if speed > 0.3 {
            return Doing::Moving;
        }
        return match l.act {
            Act::Graze => Doing::Eating,
            Act::Drink => Doing::Drinking,
            Act::Sit | Act::Lie => Doing::Resting,
            Act::Walk => Doing::Moving,
            Act::Stand | Act::Look => Doing::Standing,
        };
    }
    if let Some(f) = flit {
        return if f.perched() { Doing::Perched } else { Doing::Flying };
    }
    if let Some(f) = flyer {
        return if f.perched { Doing::Perched } else { Doing::Flying };
    }
    if let Some(f) = anim.and_then(|a| a.flight.as_ref()) {
        return if f.fold > 0.5 { Doing::Perched } else { Doing::Flying };
    }
    if speed > 0.3 {
        Doing::Moving
    } else {
        Doing::Standing
    }
}

/// Rays on a grid across the frame: does any meet water?
fn water_in_frame(view: &View, frame: [f32; 4], store: &Store, sea: f32) -> bool {
    for j in 0..6 {
        for i in 0..8 {
            let d = view.ray((i as f32 + 0.5) / 8.0, (j as f32 + 0.5) / 6.0, frame);
            let hit = store.ray(view.eye, d, 300.0, |m| m != crate::world::material::AIR);
            let sea_t = if d.y < -1e-4 && view.eye.y > sea { (view.eye.y - sea) / -d.y } else { f32::INFINITY };
            match hit {
                Some(h) if h.t < sea_t => {
                    if crate::creatures::terrain::water_voxel(h.mat) {
                        return true;
                    }
                }
                _ if sea_t < 300.0 => return true,
                _ => {}
            }
        }
    }
    false
}
