//! What comes down out of the trees on its own: leaves in autumn, blossom in
//! spring, seeds on a summer breeze.
//!
//! Each try picks a column near the camera and looks down it for a crown.
//! Only a tree that is wearing the right thing drops anything: a leaf falls
//! from a voxel of some plant's autumn colour, a petal from some plant's
//! blossom, so a pine wood stays still in October and a turning wood drops
//! leaves tree by tree as the turn walks through it. The particle wears the
//! material it fell from.

use super::Fx;
use crate::world::material::AIR;
use crate::world::matlib::{self, flag};
use crate::world::season::Season;
use crate::world::store::Store;
use glam::Vec3;

/// How far from the camera a leaf may start, metres.
const REACH: f32 = 18.0;
/// How far above and below the eye a column is searched, metres.
const ABOVE: f32 = 18.0;
const BELOW: f32 = 8.0;

/// Tries a second in each season, before the wind has its say.
fn rate(s: Season) -> f32 {
    match s {
        Season::Autumn => 80.0,
        Season::Spring => 40.0,
        Season::Summer => 8.0,
        Season::Winter => 0.0,
    }
}

/// Which materials drop what, worked out once per library.
#[derive(Clone, Debug)]
pub struct State {
    /// Some plant's autumn leaf.
    autumn: Vec<bool>,
    /// Some plant's blossom (`leaf_alt`).
    blossom: Vec<bool>,
    owed: f32,
}

impl Default for State {
    fn default() -> Self {
        State {
            autumn: vec![false; 256],
            blossom: vec![false; 256],
            owed: 0.0,
        }
    }
}

impl State {
    pub fn of(lib: &crate::library::Library) -> State {
        let mut s = State::default();
        for (pi, p) in lib.plants.iter().enumerate() {
            if let Some(m) = p.autumn.as_deref().and_then(|n| lib.material(n)) {
                s.autumn[m as usize] = true;
            }
            // A plant whose coats turn different colours (crimson, orange and
            // gold maples) drops leaves of each.
            for (which, c) in lib.plant_coats(pi).iter().enumerate() {
                for k in 0..c.shades.max(1) {
                    if let Some(m) = lib.worn(pi, which, k).get("autumn") {
                        s.autumn[m as usize] = true;
                    }
                }
            }
            if let Some(m) = p.leaf_alt.as_ref().and_then(|(n, _)| lib.material(n)) {
                s.blossom[m as usize] = true;
            }
        }
        s
    }
}

/// One step of falling leaves round the eye. `wind` is the weather's
/// multiplier (about 0 to 1.5).
pub fn step(fx: &mut Fx, store: &Store, eye: Vec3, season: Season, wind: f32, dt: f32) {
    if !fx.on {
        return;
    }
    let r = rate(season) * (0.5 + 0.5 * wind.clamp(0.0, 2.0));
    fx.ambient.owed += r * dt;
    // A frame that came late does not dump a pile of leaves at once.
    fx.ambient.owed = fx.ambient.owed.min(8.0);
    while fx.ambient.owed >= 1.0 {
        fx.ambient.owed -= 1.0;
        let (a, d) = (fx.roll() * std::f32::consts::TAU, REACH * fx.roll().sqrt());
        let (x, z) = (eye.x + a.cos() * d, eye.z + a.sin() * d);
        let (vx, vz) = ((x * 4.0).floor() as i32, (z * 4.0).floor() as i32);
        let top = ((eye.y + ABOVE) * 4.0) as i32;
        let bottom = ((eye.y - BELOW) * 4.0) as i32;
        // Every other voxel down the column: a crown is never one voxel thick.
        let mut vy = top;
        while vy > bottom {
            let Some(m) = store.get(vx, vy, vz) else {
                vy -= 32;
                continue;
            };
            if m == AIR {
                vy -= 2;
                continue;
            }
            let at = Vec3::new(x, vy as f32 / 4.0 - 0.1, z);
            let down = Vec3::NEG_Y;
            match season {
                Season::Autumn if fx.ambient.autumn[m as usize] => fx.burst("leaf_fall", at, down, m, 1.0),
                Season::Spring if fx.ambient.blossom[m as usize] => fx.burst("petal_fall", at, down, m, 1.0),
                Season::Summer if matlib::has(m, flag::FOLIAGE) || matlib::has(m, flag::GROWS) => fx.burst("seed_drift", at, down, 0, 1.0),
                _ => {}
            }
            break;
        }
    }
}
