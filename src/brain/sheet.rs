//! `--bot-sheet`: the bots' bodies frozen in a row of states, for judging
//! how they move without watching a match.
//!
//! Two rows of the same states: the top seen from the side, the bottom
//! turned three-quarters toward the camera. Each figure is driven through
//! `Figure::step` exactly as a match drives it, for long enough to settle,
//! and drawn by `shooter::draw_bot`, so what the sheet shows is what a match
//! draws. The builds and the guns go round along the row, so one sheet shows
//! all three bodies and most of the arsenal.

use super::avatar::{self, Avatar};
use crate::anim::figure::{Drive, Figure, Walk};
use crate::combat::viewmodel;
use crate::models::{Instance, Model};
use glam::{Quat, Vec3};

/// The states along the row, left to right.
pub const STATES: [&str; 10] = ["idle", "walk", "run", "sprint", "backpedal", "strafe", "aim up", "aim down", "jump", "firing"];

/// Metres between figures along a row, and from the camera to the rows.
const SPACING: f32 = 1.15;
const DEPTH: f32 = 6.2;

pub struct Sheet {
    cells: Vec<Cell>,
    zombies: bool,
}

/// `--zombie-sheet`'s columns: a style and how fast it is going.
pub const ZOMBIE_STATES: [(Walk, f32); 9] = [
    (Walk::Shamble, 0.0),
    (Walk::Shamble, 1.1),
    (Walk::Lurch, 0.0),
    (Walk::Lurch, 1.6),
    (Walk::Runner, 3.0),
    (Walk::Runner, 6.5),
    (Walk::Crawler, 0.0),
    (Walk::Crawler, 0.8),
    (Walk::Upright, 1.6),
];

struct Cell {
    build: usize,
    figure: Figure,
    at: Vec3,
    yaw: f32,
    gun: Option<usize>,
    tint: [f32; 3],
}

/// Pose every figure in front of a camera at `eye` looking along `yaw`.
///
/// With `guns` (shapes, from `--sheet-guns`) the columns are those guns
/// instead of the states: each bot holds one the way it carries it, still
/// on the top row and swinging or firing on the bottom.
pub fn spawn(av: &Avatar, eye: Vec3, yaw: f32, guns: &[usize]) -> Sheet {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mid = eye + fwd * DEPTH;
    let n = if guns.is_empty() { STATES.len() } else { guns.len() };
    // A row of long guns side on needs more room than a row of states.
    let spacing = if guns.is_empty() { SPACING } else { 1.75 };
    let mut cells = Vec::new();
    // Side on, facing right across the screen; then three-quarters toward
    // the camera, turned a little to its right.
    for (row, (body_yaw, lift)) in [(yaw - std::f32::consts::FRAC_PI_2, 0.2), (yaw + std::f32::consts::PI - 0.6, -2.0)].into_iter().enumerate() {
        for c in 0..n {
            let (state, gun) = match guns.get(c) {
                Some(g) => (if row == 0 { "idle" } else { "firing" }, *g),
                None => (STATES[c], [1, 9, 3, 6, 7, 0, 8, 2, 4, 5][c % 10] % viewmodel::SHAPE_COUNT),
            };
            let build = c % av.bots.max(1);
            let body = av.body(build);
            let at = mid + right * ((c as f32 - (n as f32 - 1.0) * 0.5) * spacing) + Vec3::Y * lift;
            let mut figure = body.figure(1 + c as u32 * 7 + row as u32 * 101);
            drive(&mut figure, body, state, body_yaw, gun);
            cells.push(Cell {
                build,
                figure,
                at,
                yaw: body_yaw,
                gun: Some(gun),
                tint: avatar::team_tint(c),
            });
        }
    }
    Sheet { cells, zombies: false }
}

/// `--zombie-sheet`: the zombie builds in each way of walking
/// (`ZOMBIE_STATES`), standing and moving, the builds going round along the
/// row. The same two rows as the bots' sheet.
pub fn spawn_zombies(av: &Avatar, eye: Vec3, yaw: f32) -> Sheet {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mid = eye + fwd * DEPTH;
    let n = ZOMBIE_STATES.len();
    let zombies = (av.bodies.len() - av.bots).max(1);
    let mut cells = Vec::new();
    for (row, (body_yaw, lift)) in [(yaw - std::f32::consts::FRAC_PI_2, 0.2), (yaw + std::f32::consts::PI - 0.6, -2.0)].into_iter().enumerate() {
        for (c, &(walk, speed)) in ZOMBIE_STATES.iter().enumerate() {
            let build = av.bots + (c + row) % zombies;
            let body = av.body(build);
            let at = mid + right * ((c as f32 - (n as f32 - 1.0) * 0.5) * 1.3) + Vec3::Y * lift;
            let mut figure = body.figure(1 + c as u32 * 7 + row as u32 * 101);
            figure.walk = walk;
            let way = Vec3::new(-body_yaw.sin(), 0.0, -body_yaw.cos());
            let d = Drive { yaw: body_yaw, vel: way * speed, holding: false, ..Default::default() };
            for _ in 0..67 {
                figure.step(&body.rig, body.voxel, &d);
            }
            cells.push(Cell { build, figure, at, yaw: body_yaw, gun: None, tint: [0.62, 0.7, 0.55] });
        }
    }
    Sheet { cells, zombies: true }
}

/// Step a figure through `state` until it has settled into it.
fn drive(f: &mut Figure, body: &avatar::Body, state: &str, yaw: f32, gun: usize) {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let base = Drive { yaw, grasp: viewmodel::grasp(gun), ..Default::default() };
    let d = match state {
        "walk" => Drive { vel: fwd * 2.5, ..base },
        "run" => Drive { vel: fwd * 6.0, ..base },
        "sprint" => Drive { vel: fwd * 7.6, sprint: true, ..base },
        "backpedal" => Drive { vel: -fwd * 3.0, ..base },
        "strafe" => Drive { vel: right * 4.5, ..base },
        "aim up" => Drive { pitch: 0.7, ..base },
        "aim down" => Drive { pitch: -0.6, ..base },
        _ => base,
    };
    // Long enough for every smoothed quantity to arrive, and an odd count
    // so the strides are caught mid-step rather than feet together.
    for i in 0..67 {
        // A swing is caught at its height, a gun mid-burst.
        if state == "firing" && (i % 4 == 0 || i == 64) {
            f.fired(9.0);
        }
        f.step(&body.rig, body.voxel, &d);
    }
    if state == "jump" {
        // Off the ground and still rising.
        let up = Drive {
            vel: fwd * 3.0 + Vec3::Y * 4.0,
            on_ground: false,
            ..base
        };
        for _ in 0..5 {
            f.step(&body.rig, body.voxel, &up);
        }
    }
}

impl Sheet {
    /// What to draw. `base` is where the guns' models start
    /// (`Game::weapon_models`).
    pub fn instances(&self, av: &Avatar, base: usize, models: &[Model], out: &mut Vec<(usize, Instance)>) {
        let mut fk: Vec<(Vec3, Quat)> = Vec::new();
        for c in &self.cells {
            let tint = [c.tint[0], c.tint[1], c.tint[2], 0.0];
            let lit = avatar::lamp_colour(c.tint, 1.0);
            crate::shooter::draw_bot(av.body(c.build), &c.figure, c.at, c.yaw, 1.0, tint, lit, c.gun, base, models, &mut fk, out);
        }
    }

    /// The states in order, for the console.
    pub fn lines(&self) -> Vec<String> {
        if self.zombies {
            let cols: Vec<String> = ZOMBIE_STATES.iter().map(|(w, v)| format!("{w} {v} m/s")).collect();
            return vec![format!("zombie sheet, left to right, side view on top: {}", cols.join(", "))];
        }
        vec![format!("bot sheet, left to right, side view on top: {}", STATES.join(", "))]
    }
}
