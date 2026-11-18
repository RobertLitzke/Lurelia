//! Voxel materials and which texture layer each face uses.

pub const AIR: u8 = 0;
pub const GRASS: u8 = 1;
pub const DIRT: u8 = 2;
pub const SAND: u8 = 3;
pub const STONE: u8 = 4;
pub const STONE_DARK: u8 = 5;
pub const STONE_LIGHT: u8 = 6;
pub const BARK: u8 = 7;
pub const LEAVES: u8 = 8;
pub const PINE: u8 = 9;
pub const PLANKS: u8 = 10;
pub const GRAVEL: u8 = 11;
pub const BIRCH_BARK: u8 = 12;
pub const LEAVES_AUTUMN: u8 = 13;
pub const SNOW: u8 = 14;
pub const BIRCH_LEAVES: u8 = 15;
pub const BRICK: u8 = 16;

/// Ground materials whose one-voxel steps the mesher turns into slopes. Trees,
/// planks and brick keep their square edges.
pub fn smooths(m: u8) -> bool {
    matches!(m, GRASS | DIRT | SAND | STONE | STONE_DARK | STONE_LIGHT | GRAVEL | SNOW) || super::matlib::smooth(m)
}

/// Thin stems the mesher bevels into octagons (vines, culms): see mesh.rs.
pub fn tubes(m: u8) -> bool {
    super::matlib::tubes(m)
}

/// Tree materials, which the mesher rounds into blobs (see mesh.rs).
pub fn rounds(m: u8) -> bool {
    matches!(m, BARK | LEAVES | PINE | BIRCH_BARK | LEAVES_AUTUMN | BIRCH_LEAVES) || super::matlib::rounds(m)
}

/// Entity models use material ids from here up; they index a flat colour palette.
pub const PALETTE_BASE: u8 = 64;

/// Texture layers, in the order textures.rs builds them.
pub mod layer {
    pub const GRASS_TOP: u32 = 0;
    pub const GRASS_SIDE: u32 = 1;
    pub const DIRT: u32 = 2;
    pub const SAND: u32 = 3;
    pub const STONE: u32 = 4;
    pub const STONE_DARK: u32 = 5;
    pub const STONE_LIGHT: u32 = 6;
    pub const BARK: u32 = 7;
    pub const BARK_TOP: u32 = 8;
    pub const LEAVES: u32 = 9;
    pub const PINE: u32 = 10;
    pub const PLANKS: u32 = 11;
    pub const GRAVEL: u32 = 12;
    pub const BIRCH_BARK: u32 = 13;
    pub const LEAVES_AUTUMN: u32 = 14;
    pub const SNOW: u32 = 15;
    pub const BIRCH_LEAVES: u32 = 16;
    pub const BRICK: u32 = 17;
    pub const COUNT: u32 = 18;
}

/// (top, side, bottom) texture layer for each material.
pub fn faces(m: u8) -> [u32; 3] {
    use layer::{GRASS_SIDE, GRASS_TOP};
    match m {
        GRASS => [GRASS_TOP, GRASS_SIDE, layer::DIRT],
        DIRT => [layer::DIRT; 3],
        SAND => [layer::SAND; 3],
        STONE => [layer::STONE; 3],
        STONE_DARK => [layer::STONE_DARK; 3],
        STONE_LIGHT => [layer::STONE_LIGHT; 3],
        BARK => [layer::BARK; 3],
        LEAVES => [layer::LEAVES; 3],
        PINE => [layer::PINE; 3],
        PLANKS => [layer::PLANKS; 3],
        GRAVEL => [layer::GRAVEL; 3],
        BIRCH_BARK => [layer::BIRCH_BARK; 3],
        LEAVES_AUTUMN => [layer::LEAVES_AUTUMN; 3],
        SNOW => [layer::SNOW, layer::SNOW, layer::DIRT],
        BIRCH_LEAVES => [layer::BIRCH_LEAVES; 3],
        BRICK => [layer::BRICK; 3],
        _ => super::matlib::faces(m).unwrap_or([layer::STONE; 3]),
    }
}

pub fn name(m: u8) -> &'static str {
    match m {
        GRASS => "GRASS",
        DIRT => "DIRT",
        SAND => "SAND",
        STONE => "STONE",
        STONE_DARK => "DARK STONE",
        STONE_LIGHT => "LIGHT STONE",
        BARK => "LOG",
        LEAVES => "LEAVES",
        PINE => "PINE",
        PLANKS => "PLANKS",
        GRAVEL => "GRAVEL",
        BIRCH_BARK => "BIRCH",
        LEAVES_AUTUMN => "AUTUMN LEAVES",
        SNOW => "SNOW",
        BIRCH_LEAVES => "BIRCH LEAVES",
        BRICK => "BRICK",
        _ => super::matlib::name(m).unwrap_or("?"),
    }
}
