//! What the subject is doing while you edit it.
//!
//! The page used to spawn the entry into the world as a living animal with
//! its mover and its mind, which meant the thing you had come to look at
//! grazed out of frame and had to be dragged back by respawning it. Here the
//! studio asks for a pose instead: the model is stood still at the studio
//! spot with no body and no mind, and its rig is driven directly, so a walk
//! is walked on the spot and a sleeping animal stays asleep.
//!
//! LIVE is the old behaviour, kept because watching one graze in its biome is
//! worth having. Everything else is a pose this module drives.

use crate::anim::{flight, life, Anim};
use crate::creatures::locomotion::Verb;
use crate::creatures::Species;
use crate::models::Model;
use glam::Vec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pose {
    /// Spawned into the world and left to itself.
    Live,
    /// The rig's own rest pose, held. The one pose anything can take.
    Rest,
    Stand,
    Walk,
    Run,
    Graze,
    Look,
    Sit,
    Lie,
    /// Birds: on the wing, and down off it.
    Fly,
    Perch,
}

impl Pose {
    pub fn name(self) -> &'static str {
        match self {
            Pose::Live => "live",
            Pose::Rest => "rest",
            Pose::Stand => "stand",
            Pose::Walk => "walk",
            Pose::Run => "run",
            Pose::Graze => "graze",
            Pose::Look => "look about",
            Pose::Sit => "sit",
            Pose::Lie => "lie down",
            Pose::Fly => "fly",
            Pose::Perch => "perch",
        }
    }

    /// The activity a walker's rig is pinned to.
    fn act(self) -> Option<life::Act> {
        Some(match self {
            Pose::Stand => life::Act::Stand,
            Pose::Walk | Pose::Run => life::Act::Walk,
            Pose::Graze => life::Act::Graze,
            Pose::Look => life::Act::Look,
            Pose::Sit => life::Act::Sit,
            Pose::Lie => life::Act::Lie,
            _ => return None,
        })
    }
}

/// The poses worth offering for this creature: what its rig can actually do.
/// A model with no rig is one lump of voxels, so it has only the two that
/// need no bones.
pub fn choices(verb: &Verb, rig: bool) -> Vec<Pose> {
    let mut out = vec![Pose::Live, Pose::Rest];
    if !rig {
        return out;
    }
    if verb.flies() {
        out.extend([Pose::Fly, Pose::Perch]);
    } else {
        out.extend([Pose::Stand, Pose::Walk, Pose::Run, Pose::Graze, Pose::Look, Pose::Sit, Pose::Lie]);
    }
    out
}

/// The pose to fall back on when the one in force does not fit the entry
/// being shown (a bird after a deer).
pub fn nearest(want: Pose, verb: &Verb, rig: bool) -> Pose {
    let all = choices(verb, rig);
    if all.contains(&want) {
        return want;
    }
    // A gait asked for on a bird is a flight, and the other way about.
    let swap = match want {
        Pose::Walk | Pose::Run | Pose::Graze | Pose::Look => Pose::Fly,
        Pose::Stand | Pose::Sit | Pose::Lie => Pose::Perch,
        Pose::Fly => Pose::Walk,
        Pose::Perch => Pose::Stand,
        p => p,
    };
    if all.contains(&swap) {
        swap
    } else {
        Pose::Rest
    }
}

/// Build the animation the pose wants, or None where the model has no rig to
/// pose (it is then drawn as it stands).
pub fn build(pose: Pose, sp: &Species, model: &Model, seed: u32) -> Option<Anim> {
    let rig = model.rig.as_ref()?;
    if pose == Pose::Rest || pose == Pose::Live {
        return Some(Anim::hold(crate::anim::Pose::rest(rig.bones.len())));
    }
    let verb = sp.verb();
    if verb.flies() {
        let pattern = sp.beh.flight.as_deref().and_then(flight::Pattern::parse).unwrap_or(verb.flight);
        let beat = sp.beh.wingbeat.unwrap_or_else(|| flight::beat_for(rig, model.voxel));
        return Some(Anim::flying(rig, flight::Flight::new(pattern, beat, seed | 1)));
    }
    let mut a = Anim::alive(rig, rig.table(verb.routine), seed);
    if let Some(l) = a.life.as_mut() {
        l.on_side = sp.beh.sleeps_on_side;
        if let Some(act) = pose.act() {
            l.act = act;
        }
    }
    Some(a)
}

/// Advance the pose one frame. The subject never moves: a walk is walked on
/// the spot, which is what a studio is for.
pub fn drive(anim: &mut Anim, model: &Model, sp: &Species, pose: Pose, at: Vec3, yaw: f32, dt: f32) {
    let Some(rig) = model.rig.as_ref() else { return };
    if pose == Pose::Rest {
        return;
    }
    if anim.flight.is_some() {
        let perched = pose == Pose::Perch;
        // A bird on the wing is given the speed it would be making, so the
        // wings beat as they would in the air rather than hanging folded.
        let vel = if perched {
            Vec3::ZERO
        } else {
            Vec3::new(-yaw.sin(), 0.0, -yaw.cos()) * sp.beh.walk_speed.max(4.0)
        };
        anim.step_flight(
            rig,
            model.voxel,
            &flight::Input {
                dt,
                vel,
                yaw,
                perched,
                landing: false,
                cling: false,
            },
        );
        return;
    }
    let moving = matches!(pose, Pose::Walk | Pose::Run);
    let speed = match pose {
        Pose::Walk => sp.beh.walk_speed,
        Pose::Run => sp.beh.flee_speed,
        _ => 0.0,
    };
    if let (Some(l), Some(act)) = (anim.life.as_mut(), pose.act()) {
        // Pinned every frame: the scheduler would otherwise wander off to
        // whatever its table says next, and the page asked for this one.
        l.act = act;
    }
    let ground = |_: f32, _: f32| Some(at.y);
    anim.step(
        rig,
        model.voxel,
        &life::Input {
            dt,
            speed,
            moving,
            alarm: pose == Pose::Run,
            on_ground: true,
            pos: at,
            yaw,
            // Nobody is watching it: the page is about the pose it was asked
            // for, not about what the camera makes it do.
            player: at + Vec3::X * 1.0e4,
            wary: 0.0,
            ground: &ground,
            water: None,
            hide: false,
        },
    );
}
