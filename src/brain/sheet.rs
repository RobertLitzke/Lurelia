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
use crate::anim::figure::{Drive, Figure};
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
}

struct Cell {
    build: usize,
    figure: Figure,
    at: Vec3,
    yaw: f32,
    gun: usize,
    tint: [f32; 3],
}

/// Pose every figure in front of a camera at `eye` looking along `yaw`.
pub fn spawn(av: &Avatar, eye: Vec3, yaw: f32) -> Sheet {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let mid = eye + fwd * DEPTH;
    let n = STATES.len();
    let mut cells = Vec::new();
    // Side on, facing right across the screen; then three-quarters toward
    // the camera, turned a little to its right.
    for (row, (body_yaw, lift)) in [(yaw - std::f32::consts::FRAC_PI_2, 0.2), (yaw + std::f32::consts::PI - 0.6, -2.0)].into_iter().enumerate() {
        for (c, state) in STATES.iter().enumerate() {
            let build = c % av.bodies.len();
            let body = av.body(build);
            let at = mid + right * ((c as f32 - (n as f32 - 1.0) * 0.5) * SPACING) + Vec3::Y * lift;
            let mut figure = body.figure(1 + c as u32 * 7 + row as u32 * 101);
            drive(&mut figure, body, state, body_yaw);
            cells.push(Cell {
                build,
                figure,
                at,
                yaw: body_yaw,
                gun: [1, 9, 3, 6, 7, 0, 8, 2, 4, 5][c % 10] % viewmodel::SHAPES.len(),
                tint: avatar::team_tint(c),
            });
        }
    }
    Sheet { cells }
}

/// Step a figure through `state` until it has settled into it.
fn drive(f: &mut Figure, body: &avatar::Body, state: &str, yaw: f32) {
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    let base = Drive { yaw, ..Default::default() };
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
        if state == "firing" && i % 4 == 0 {
            f.fired(9.0);
        }
        f.step(&body.rig, body.voxel, &d);
    }
    if state == "jump" {
        // Off the ground and still rising.
        let up = Drive { vel: fwd * 3.0 + Vec3::Y * 4.0, on_ground: false, ..base };
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
            crate::shooter::draw_bot(av.body(c.build), &c.figure, c.at, c.yaw, 1.0, tint, lit, Some(c.gun), base, models, &mut fk, out);
        }
    }

    /// The states in order, for the console.
    pub fn lines(&self) -> Vec<String> {
        vec![format!("bot sheet, left to right, side view on top: {}", STATES.join(", "))]
    }
}
