//! Shader sources, assembled in one place. Every module that needs them
//! starts with a prelude of constants generated from the Rust values, so the
//! two sides can't drift apart, and `tests` parses and validates every module
//! so a WGSL mistake fails `cargo test` instead of every run of the game.

use super::{foliage, hud, SHADOW_SIZE};
use crate::mesh::quad;
use crate::world::material::PALETTE_BASE;
use crate::world::matlib::flag;

const COMMON: &str = include_str!("../shaders/common.wgsl");
const SCENE: &str = include_str!("../shaders/scene.wgsl");
const SHADOW: &str = include_str!("../shaders/shadow.wgsl");
const FOLIAGE: &str = include_str!("../shaders/foliage.wgsl");
const PARTICLES: &str = include_str!("../shaders/particles.wgsl");
const POST: &str = include_str!("../shaders/post.wgsl");
const HUD: &str = include_str!("../shaders/hud.wgsl");
const OCCLUSION: &str = include_str!("../shaders/occlusion.wgsl");

/// Constants the shaders share with Rust.
/// A comparison switch: on unless its variable is set to "0".
fn env_on(var: &str) -> bool {
    std::env::var(var).map(|v| v != "0").unwrap_or(true)
}

fn prelude() -> String {
    let v2 = |v: [f32; 2]| format!("vec2<f32>({:?}, {:?})", v[0], v[1]);
    let w = crate::world::WIND;
    [
        "// Generated in render/shaders.rs from the Rust constants.".to_string(),
        format!("const SHADOW_SIZE: f32 = {:?};", SHADOW_SIZE as f32),
        format!("const PALETTE_BASE: u32 = {PALETTE_BASE}u;"),
        format!("const MAP_GLYPH: u32 = {}u;", hud::MAP),
        format!("const BACKDROP_GLYPH: u32 = {}u;", hud::BACKDROP),
        format!("const LOGO_GLYPH: u32 = {}u;", hud::LOGO),
        format!("const TEXT_GLYPH: u32 = {}u;", hud::TEXT),
        format!("const GRAD_H_GLYPH: u32 = {}u;", hud::GRAD_H),
        format!("const ATLAS_SIZE: vec2<f32> = {};", v2([super::font::font().width as f32, super::font::font().height as f32])),
        format!("const ATLAS_SPREAD: f32 = {:?};", super::font::font().spread),
        format!("const LOGO_SIZE: vec2<f32> = {};", v2([super::font::logo_size().0 as f32, super::font::logo_size().1 as f32])),
        format!("const GRASS_FADE: vec2<f32> = {};", v2(foliage::GRASS_FADE)),
        format!("const LEAF_FADE: vec2<f32> = {};", v2(foliage::LEAF_FADE)),
        format!("const STALK_FADE: vec2<f32> = {};", v2(foliage::STALK_FADE)),
        format!("const FILL_FADE: vec2<f32> = {};", v2(foliage::FILL_FADE)),
        format!("const PATCH_FADE: vec2<f32> = {};", v2(foliage::PATCH_FADE)),
        format!("const WIND: vec2<f32> = {};", v2([w.x, w.y])),
        // How a card's growing-in is spread over its fade (foliage.wgsl
        // grow). Below 1 it rises fast while the card is far off and eases
        // as you walk up; VOXEL_GROW_CURVE=1 is the old straight ramp.
        format!("const GROW_CURVE: f32 = {:?};", std::env::var("VOXEL_GROW_CURVE").ok().and_then(|v| v.parse().ok()).unwrap_or(0.4f32)),
        // The baked skyline map (render/horizon.rs): its shadow and its sky
        // occlusion, and the shadow the clouds throw (scene.wgsl). All three
        // are what the distance is lit by, so each has a switch for
        // comparisons: VOXEL_HORIZON=0, VOXEL_SKY_OCCLUSION=0, VOXEL_CLOUD_SHADOW=0.
        format!("const HORIZON_AZIMUTHS: i32 = {};", super::horizon::AZIMUTHS),
        format!("const HORIZON_ON: bool = {};", env_on("VOXEL_HORIZON")),
        format!("const SKY_OCCLUSION_ON: bool = {};", env_on("VOXEL_SKY_OCCLUSION")),
        format!("const CLOUD_SHADOW_ON: bool = {};", env_on("VOXEL_CLOUD_SHADOW")),
        // The sun stopped at the land's surface, so it does not reach through
        // rock into a cave or a buried room (scene.wgsl under_land);
        // VOXEL_SUNK_SHADOW=0 for comparisons.
        format!("const SUNK_ON: bool = {};", env_on("VOXEL_SUNK_SHADOW")),
        // How much of the sky fill a closed-in hollow loses.
        format!("const SKY_OPENNESS: f32 = {:?};", 0.55f32),
        // The grass understory (scene.wgsl); VOXEL_UNDERSTORY=0 for comparisons.
        format!("const UNDER_ON: bool = {};", std::env::var("VOXEL_UNDERSTORY").map(|v| v != "0").unwrap_or(true)),
        format!("const M_FOLIAGE_FW: u32 = {}u;", flag::FOLIAGE),
        format!("const M_GROWS: u32 = {}u;", flag::GROWS),
        // Distant canopies broken into clumps of sun and shade (scene.wgsl);
        // VOXEL_DAPPLE=0 for comparisons.
        format!("const DAPPLE_ON: bool = {};", std::env::var("VOXEL_DAPPLE").map(|v| v != "0").unwrap_or(true)),
        // Per-voxel jitter for the classic style's texture set (render/texsets.rs).
        format!("const TEX_JITTER: f32 = {:?};", super::texsets::JITTER),
        // Material flags in mats[m].w (world/matlib.rs flag).
        format!("const M_GLINTS: u32 = {}u;", flag::GLINTS),
        format!("const M_GLINTS_SOFT: u32 = {}u;", flag::GLINTS_SOFT),
        format!("const M_CALM_MEADOW: u32 = {}u;", flag::CALM_MEADOW),
        format!("const M_CALM_LEAF: u32 = {}u;", flag::CALM_LEAF),
        format!("const M_CLASSIC_PATCH: u32 = {}u;", flag::CLASSIC_PATCH),
        format!("const M_UNDERSTORY: u32 = {}u;", flag::UNDERSTORY),
        format!("const M_GRAIN_X: u32 = {}u;", flag::GRAIN_X),
        format!("const M_GRAIN_Z: u32 = {}u;", flag::GRAIN_Z),
        format!("const M_FACET: u32 = {}u;", flag::FACET),
        format!("const M_GLOW_SPECKS: u32 = {}u;", flag::GLOW_SPECKS),
        format!("const M_NIGHT_GLOW: u32 = {}u;", flag::NIGHT_GLOW),
        format!("const L_GLOW_THREADS: u32 = {}u;", crate::mesh::sprites::layer::GLOW_THREADS),
        format!("const M_GLOW_SHIFT: u32 = {}u;", flag::GLOW_SHIFT),
        // The packed quad's fields (mesh/quad.rs).
        format!("const Q_Y_SHIFT: u32 = {}u;", quad::Y_SHIFT),
        format!("const Q_Z_SHIFT: u32 = {}u;", quad::Z_SHIFT),
        format!("const Q_W_SHIFT: u32 = {}u;", quad::W_SHIFT),
        format!("const Q_H_SHIFT: u32 = {}u;", quad::H_SHIFT),
        format!("const Q_DIR_SHIFT: u32 = {}u;", quad::DIR_SHIFT),
        format!("const Q_ROUND_BIT: u32 = {}u;", quad::ROUND_BIT),
        format!("const Q_NORMAL_BIT: u32 = {}u;", quad::NORMAL_BIT),
        format!("const Q_BLOCK_BIT: u32 = {}u;", quad::BLOCK_BIT),
        format!("const Q_AO_SHIFT: u32 = {}u;", quad::AO_SHIFT),
        format!("const Q_BLEND_SHIFT: u32 = {}u;", quad::BLEND_SHIFT),
    ]
    .join("\n")
}

/// One variant number (0 = off) per palette study, from VOXEL_GRADE, e.g.
/// "pal2". Only "pal" is read here now (render/daylight.rs); the rest became
/// live knobs on the LOOK page, and `Grade::apply_env` maps the old tokens
/// onto them. The saturated style's default is a pushed golden hour (pal3); VOXEL_GRADE=legacy
/// turns the lot off for comparison.
pub fn grade_variant(key: &str) -> u32 {
    let v = std::env::var("VOXEL_GRADE").unwrap_or_default();
    if v.split(',').any(|t| t.trim() == "legacy") {
        return 0;
    }
    let default = match key {
        "fog" | "sun" | "shade" => 1,
        "pal" => 3,
        _ => 0,
    };
    v.split(',').find_map(|t| t.trim().strip_prefix(key).and_then(|n| n.parse().ok())).unwrap_or(default)
}

fn join(parts: &[&str]) -> String {
    let mut s = prelude();
    for p in parts {
        s.push('\n');
        s.push_str(p);
    }
    s
}

pub fn scene() -> String {
    join(&[COMMON, SCENE])
}

pub fn shadow() -> String {
    join(&[COMMON, SHADOW])
}

/// The sprite pass builds on the scene's lighting functions.
pub fn foliage() -> String {
    join(&[COMMON, SCENE, FOLIAGE])
}

/// Particles (src/fx) are lit and fogged by the scene's functions.
pub fn particles() -> String {
    join(&[COMMON, SCENE, PARTICLES])
}

/// The depth texture type follows the MSAA setting.
pub fn post(msaa: bool) -> String {
    join(&[&POST.replace("DEPTH_TEXTURE", if msaa { "texture_depth_multisampled_2d" } else { "texture_depth_2d" })])
}

pub fn hud() -> String {
    join(&[HUD])
}

/// The occlusion pyramid and test; the depth texture follows MSAA as in post.
pub fn occlusion(msaa: bool) -> String {
    let (ty, samples, load) = if msaa {
        ("texture_depth_multisampled_2d", "4u", "textureLoad(src_depth, p, i32(s))")
    } else {
        ("texture_depth_2d", "1u", "textureLoad(src_depth, p, 0)")
    };
    join(&[&OCCLUSION.replace("DEPTH_TEXTURE", ty).replace("DEPTH_SAMPLES", samples).replace("DEPTH_LOAD", load)])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every shader module parses and validates, as the game will build it.
    #[test]
    fn shaders_validate() {
        let modules = [
            ("scene", scene()),
            ("shadow", shadow()),
            ("foliage", foliage()),
            ("particles", particles()),
            ("post msaa", post(true)),
            ("post", post(false)),
            ("hud", hud()),
            ("occlusion msaa", occlusion(true)),
            ("occlusion", occlusion(false)),
        ];
        for (name, src) in modules {
            let module = naga::front::wgsl::parse_str(&src).unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{name}: {}", e.emit_to_string(&src)));
        }
    }
}
