//! A race's polished racing line (`race/polish.rs`), drawn without touching
//! the world: a map over the whole world, a texel to the metre (`side`),
//! each saying how much darker the ground facing up there is and the height
//! of the road the darkening belongs to, so a bridge and the road under it
//! keep their own. The terrain shader reads it (`polishmap` in scene.wgsl)
//! and takes the texel from the map's own size, so the two agree whatever
//! the size is. A world with no marks has a blank map of one texel.
//!
//! `Game` keeps every mark of the race so far (`Marks`), and the renderer
//! sends the GPU only those it hasn't sent; a new race starts the list over
//! and the map is wiped.

/// Texels across the map of a world `world` metres across: one to the
/// metre, the size of a polish tile, so no two tiles share a texel. `max`
/// is the widest texture the GPU takes; a world wider than that gets
/// coarser texels.
pub fn side(world: f32, max: u32) -> u32 {
    (world.ceil().max(2.0) as u32).min(max)
}

/// One tile darkened: its corner on the ground (x, z), its side, the road's
/// height there, and how much darker it is (a share of the colour).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub at: [f32; 2],
    pub size: f32,
    pub y: f32,
    pub dark: f32,
}

/// Every mark so far, and which race they belong to.
#[derive(Default)]
pub struct Marks {
    pub race: u32,
    pub marks: Vec<Mark>,
}

impl Marks {
    /// The marks a race laid since last asked, after starting over if it did.
    pub fn take(&mut self, cleared: bool, marks: Vec<Mark>) {
        if cleared {
            self.race = self.race.wrapping_add(1);
            self.marks.clear();
        }
        self.marks.extend(marks);
    }
}

/// One texel as the shader reads it: r the darkening, g and b the road's
/// height in quarter metres, high byte first.
pub fn texel(m: &Mark) -> [u8; 4] {
    let h = (m.y * 4.0).round().clamp(0.0, 65535.0) as u32;
    [(m.dark.clamp(0.0, 1.0) * 255.0).round() as u8, (h >> 8) as u8, (h & 0xff) as u8, 255]
}

/// The texels a mark covers in a map `n` texels a side over a world
/// `world` metres across: x and z ranges, ends excluded.
pub fn texels(m: &Mark, world: f32, n: u32) -> Option<([u32; 2], [u32; 2])> {
    let k = n as f32 / world.max(1.0);
    let to = |a: f32, b: f32| {
        let lo = (a * k).floor().max(0.0) as u32;
        let hi = ((b * k).ceil().max(0.0) as u32).min(n);
        (lo < hi).then_some([lo, hi])
    };
    Some((to(m.at[0], m.at[0] + m.size)?, to(m.at[1], m.at[1] + m.size)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tile is one texel whatever the size of the world; the height comes
    /// back to a quarter metre.
    #[test]
    fn a_tile_is_a_texel_and_keeps_its_height() {
        let m = Mark { at: [100.0, 37.0], size: 1.0, y: 31.6, dark: 0.3 };
        for world in [512.0, 768.0] {
            let n = side(world, 8192);
            assert_eq!(n, world as u32);
            assert_eq!(texels(&m, world, n), Some(([100, 101], [37, 38])));
            let far = Mark { at: [world - 1.0, 0.0], ..m };
            assert_eq!(texels(&far, world, n), Some(([n - 1, n], [0, 1])));
        }
        let t = texel(&m);
        let h = ((t[1] as u32) << 8 | t[2] as u32) as f32 / 4.0;
        assert!((h - 31.5).abs() < 0.26, "{h}");
        assert_eq!(t[0], 77);
        // Off the map: nothing.
        assert_eq!(texels(&Mark { at: [-5.0, 0.0], ..m }, 512.0, 512), None);
    }
}
