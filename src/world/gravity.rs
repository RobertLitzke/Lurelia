//! A world's gravity: how hard it pulls and which way.
//!
//! A world says so in `world.json` (`"gravity": {"g": 0.3}`) and the voxel
//! `Store` carries it, since everything that falls already has the store in
//! hand: the player and the bots (player.rs), dead bodies (`physics::tumble`,
//! shooter/corpse.rs, shooter/gibs.rs), shells (combat/shot.rs) and particles
//! (fx/mod.rs). Each asks `pull(pos)` for the acceleration where it is, or
//! `drop(pos, own)` where the thing has a fall rate of its own.
//!
//! The rides (ride.rs: bike, glider, rope) and the creatures keep their own
//! tuned gravity and do not read this.

use glam::Vec3;
use serde::Deserialize;

/// What a body falls at where a world says nothing, metres a second squared.
/// Heavier than Earth's, which is what makes a jump feel quick.
pub const STANDARD: f32 = 24.0;

/// Under this share of `STANDARD` a world counts as low gravity, and a body
/// in the air is steered by its keys however fast it is going (player.rs).
const LOW: f32 = 0.6;

/// What gravity pulls toward. Only the plane is built: down is -y everywhere.
/// A point (an asteroid walked all the way round) and a surface (the inside
/// of a ring) belong here as further variants; `Gravity::down` is the one
/// place that would read them, though the collision's idea of a floor and
/// the camera's idea of up are y as well and would have to follow.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Toward {
    /// A level plane under the whole world.
    #[default]
    Plane,
}

/// The `gravity` block of a `world.json`.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Gravity {
    /// How hard it pulls, as a share of the engine's own (1, the default).
    /// 0.3 is a map in space: a jump goes three times as high and hangs.
    pub g: f32,
    pub toward: Toward,
}

impl Default for Gravity {
    fn default() -> Self {
        Self { g: 1.0, toward: Toward::Plane }
    }
}

impl Gravity {
    /// The unit vector a thing at `pos` falls along.
    pub fn down(&self, _pos: Vec3) -> Vec3 {
        match self.toward {
            Toward::Plane => Vec3::NEG_Y,
        }
    }

    /// The acceleration on a body at `pos`, metres a second squared.
    pub fn pull(&self, pos: Vec3) -> Vec3 {
        self.down(pos) * STANDARD * self.g
    }

    /// The acceleration on something with a fall rate of its own (a shell's
    /// `gravity`, a particle's): its rate scaled by this world's share.
    pub fn drop(&self, pos: Vec3, own: f32) -> Vec3 {
        self.down(pos) * own * self.g
    }

    /// Low enough that a body in the air floats and can be steered.
    pub fn low(&self) -> bool {
        self.g < LOW
    }

    /// What the world file may not say, as a sentence for the checker.
    pub fn problem(&self) -> Option<String> {
        (!(self.g.is_finite() && (0.0..=4.0).contains(&self.g))).then(|| format!("gravity g is {}, which should be between 0 and 4 (1 is the engine's own)", self.g))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_world_file_reads_a_share_and_defaults_to_the_plane() {
        let g: Gravity = serde_json::from_str(r#"{"g": 0.3}"#).unwrap();
        assert_eq!(g, Gravity { g: 0.3, toward: Toward::Plane });
        assert!(g.low());
        assert!((g.pull(Vec3::ZERO).y + STANDARD * 0.3).abs() < 1e-5);
        let g: Gravity = serde_json::from_str(r#"{"toward": "plane"}"#).unwrap();
        assert_eq!(g, Gravity::default());
        assert!(!g.low());
        // The shapes that are not built yet are refused, not ignored.
        assert!(serde_json::from_str::<Gravity>(r#"{"toward": "point"}"#).is_err());
        assert!(Gravity { g: -1.0, ..Default::default() }.problem().is_some());
    }
}
