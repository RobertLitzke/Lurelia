//! The world: voxel storage, materials, and the generators that fill them.

/// The prevailing wind, a unit vector on the ground plane (x, z): toward the
/// east and a little south. The glider's ridge lift and the swaying grass both
/// use it.
pub const WIND: glam::Vec2 = glam::Vec2::new(0.944, 0.33);

pub mod atlas;
pub mod bones;
pub mod canopy;
pub mod chronicle;
pub mod city;
pub mod clock;
pub mod damp;
pub mod lightning;
pub mod falls;
pub mod fields;
pub mod flora;
pub mod gen;
pub mod gravity;
pub mod material;
pub mod matlib;
pub mod noise;
pub mod places;
pub mod rivers;
pub mod route;
pub mod season;
pub mod shape;
pub mod spec;
pub mod stamp;
pub mod store;
pub mod weather;
