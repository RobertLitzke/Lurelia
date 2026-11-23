//! Family groups of walkers (a walking verb with a `group` block): members
//! spawned together share a `Kin` mark and keep together.
//!
//! A group is a flock on the ground. Each member reads the others it can see
//! and steers by the three boid rules -- away from whoever is inside its own
//! spacing, toward the middle of the rest, and round to the way they are
//! facing -- with the weights coming from the species' `group` block. A herd
//! of thirty wildebeest holds its own footprint instead of spreading until
//! each animal is alone.
//!
//! Distances are between bodies, not feet: each member is the capsule
//! crowd.rs pushes apart, so `spacing` is the room between two spines and
//! a nine-metre triceratops keeps its horns out of the one ahead's rump
//! the way a deer keeps its nose out of another's flank.

use super::crowd::Shape;
use super::walk::{Critter, Mood};
use super::{Creature, Look, Species};
use crate::util::FxHashMap as HashMap;
use glam::Vec3;

/// Marks a walking creature as one of a family group.
pub struct Kin {
    pub group: u32,
}

/// How hard a neighbour inside the spacing pushes, against the pull of the
/// group's middle. Below 1 the herd packs in; well above it, it never settles.
const SEPARATION: f32 = 2.2;
/// How fast a member turns toward where it wants to be, radians a second
/// per unit of steering.
const TURN: f32 = 1.1;
/// A member drifting this much further out than the herd's own footprint
/// stops grazing and walks back in.
const STRAY: f32 = 1.35;

/// One member as its neighbours see it.
struct Member {
    e: hecs::Entity,
    pos: Vec3,
    dir: Vec3,
    walking: bool,
    shape: Shape,
}

struct Group {
    members: Vec<Member>,
    /// Where the first of them bolted from, and which way.
    flee: Option<(Vec3, f32)>,
}

/// Steer every group: boids among themselves, and when one bolts the rest
/// within reach bolt the same way.
pub fn update(world: &mut hecs::World, species: &[Species], models: &[crate::models::Model], dt: f32) {
    let mut groups: HashMap<u32, Group> = HashMap::default();
    for (e, kin, crit, look, body, morph) in world.query::<(hecs::Entity, &Kin, &Critter, &Look, &crate::physics::Body, Option<&crate::variety::Morph>)>().iter() {
        let g = groups.entry(kin.group).or_insert(Group { members: Vec::new(), flee: None });
        g.members.push(Member {
            e,
            pos: look.pos,
            dir: Vec3::new(crit.heading.sin(), 0.0, crit.heading.cos()),
            walking: crit.mood == Mood::Walk,
            shape: Shape::of_walker(look, body, models, morph),
        });
        if crit.mood == Mood::Flee && g.flee.is_none() {
            g.flee = Some((look.pos, crit.heading));
        }
    }
    for (ent, kin, crit, look, me) in world.query_mut::<(hecs::Entity, &Kin, &mut Critter, &Look, Option<&Creature>)>() {
        let Some(g) = groups.get(&kin.group) else {
            continue;
        };
        if g.members.len() < 2 {
            continue;
        }
        let grp = me.and_then(|c| species[c.species].beh.group.clone()).unwrap_or_default();
        if let Some((at, heading)) = g.flee {
            if crit.mood != Mood::Flee && (look.pos - at).length() < grp.range * 1.5 {
                crit.mood = Mood::Flee;
                crit.heading = heading + ((crit.rng.state & 255) as f32 / 255.0 - 0.5) * 0.4;
                crit.timer = 1.4;
            }
            continue;
        }
        if matches!(crit.mood, Mood::Flee | Mood::Chase | Mood::Feed) {
            continue;
        }
        let spacing = grp.spacing.max(0.5);
        let Some(mine) = g.members.iter().find(|m| m.e == ent).map(|m| m.shape) else {
            continue;
        };
        // What this one sees of the others, all on the flat: who is too
        // close, where the middle of them is, and which way they face.
        let (mut push, mut centre, mut dir, mut n) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0f32);
        for m in &g.members {
            if m.e == ent {
                continue;
            }
            let to = Vec3::new(m.pos.x - look.pos.x, 0.0, m.pos.z - look.pos.z);
            if to.length() > grp.range {
                continue;
            }
            n += 1.0;
            centre += m.pos;
            if m.walking {
                dir += m.dir;
            }
            // Too close is measured spine to spine: for a deer that is feet
            // to feet, for a sauropod it is the gap along its whole length.
            let (p, q) = mine.nearest(&m.shape);
            let gap = q - p;
            let d = gap.length();
            if d < spacing {
                let away = if d > 0.05 { gap / d } else { (m.shape.mid() - mine.mid()).normalize_or_zero() };
                push -= Vec3::new(away.x, 0.0, away.y) * (1.0 - d / spacing);
            }
        }
        if n < 1.0 {
            continue;
        }
        let centre = centre / n;
        let to_centre = Vec3::new(centre.x - look.pos.x, 0.0, centre.z - look.pos.z);
        let out = to_centre.length();
        // How wide this many animals stand when they are together: the
        // spacing, loosened by how many there are and how weakly they hold.
        let footprint = spacing * (1.2 + n.sqrt() * 0.45) / grp.cohesion.max(0.2);
        let pull = (out / footprint).min(1.6) * grp.cohesion;
        let want = push * SEPARATION + to_centre.normalize_or_zero() * pull + dir.normalize_or_zero() * grp.alignment * 0.7;
        let force = want.length();
        if force < 0.05 {
            continue;
        }
        // A grazer that has drifted right out of the herd walks back in;
        // otherwise it keeps doing what it was doing, aimed a little better.
        if out > footprint * STRAY && crit.mood == Mood::Idle {
            crit.mood = Mood::Walk;
            crit.timer = crit.timer.max(1.0);
        }
        let heading = want.x.atan2(want.z);
        crit.heading = crate::util::turn_toward(crit.heading, heading, (dt * TURN * force).min(1.0));
    }
}
