//! `--anim-sheet NAME[:moves]`: a rigged creature side-on in two rows floating
//! above eye height in front of the camera, for headless screenshots.
//!
//! - `NAME` (bones): each column bends one bone, the lower row in pitch and
//!   the upper row in yaw, rest first. A wrong joint or a part owning the
//!   wrong voxels shows at a glance.
//! - `NAME:moves`: the lower row is walk, trot and gallop at four phases each;
//!   the upper row the postures and overlays, each composed by `Life` exactly
//!   as in the game.
//! - `NAME:family`: the creature with its parent and every relative loaded in
//!   this world (at rest, then mid-stride).
//! - `NAME:herd`: twenty individuals, each with its own size, coat and
//!   habits, then the species' rare morphs.
//! - `NAME:ages`: the four ages side by side (creatures/age.rs) — juvenile,
//!   adult and aged standing and mid-stride, then the skeleton settled from
//!   three different falls, and the side-sleeping pose if it has one.

use crate::anim::Plan;
use super::life::{Act, Life};
use super::motors::{self, Gait};
use super::{Anim, Pose, Rig};
use crate::creatures::locomotion::Verb;
use crate::entities::{Entities, Look};
use crate::models::Model;
use crate::variety::Morph;
use glam::Vec3;

/// How far each bone bends in the bones sheet, radians.
pub const BEND: f32 = 0.7;

/// One column of a sheet: which model, in what pose, and an individual's
/// variation if it has one.
struct Cell {
    label: String,
    model: usize,
    pose: Option<Pose>,
    morph: Option<Morph>,
    /// Set on a column that is drawn as bones rather than as an animal.
    age: Option<crate::creatures::age::Age>,
}

fn cells(model: usize, row: Vec<(String, Pose)>) -> Vec<Cell> {
    row.into_iter()
        .map(|(label, pose)| Cell {
            label,
            model,
            pose: Some(pose),
            morph: None,
            age: None,
        })
        .collect()
}

/// Spawn the sheet; returns lines naming each column.
pub fn spawn(ents: &mut Entities, models: &[Model], what: &str, eye: Vec3, yaw: f32) -> Result<Vec<String>, String> {
    let (name, mode) = what.split_once(':').unwrap_or((what, "bones"));
    let si = ents
        .species
        .iter()
        .position(|s| s.name == name)
        .ok_or_else(|| format!("no creature {name} in this world (is it listed by one of its biomes?)"))?;
    let sp = &ents.species[si];
    let m = &models[sp.model];
    let model = sp.model;
    let need_rig = || m.rig.as_ref().ok_or_else(|| format!("{name} has no rig"));
    let rows: [Vec<Cell>; 2] = match mode {
        "bones" => {
            let [a, b] = bones(need_rig()?);
            [cells(model, a), cells(model, b)]
        }
        "moves" => {
            let [a, b] = moves(need_rig()?, m.voxel, sp.verb(), &sp.beh);
            [cells(model, a), cells(model, b)]
        }
        "family" => family(ents, models, si),
        "herd" => herd(ents, si),
        "ages" => ages(ents, models, si),
        other => return Err(format!("unknown sheet {other}: use bones, moves, family, herd or ages")),
    };
    // Spacing from the largest creature on the sheet.
    let big = rows.iter().flatten().map(|c| models[c.model].voxel * c.morph.as_ref().map_or(1.0, |x| x.scale)).fold(m.voxel, f32::max);
    // Side-on: the body's length (model z) runs along the camera's right.
    // A crab's legs and an octopus's arms fill the whole grid that way.
    let sprawls = m.rig.as_ref().is_some_and(|r| matches!(r.plan, Some(Plan::Crab | Plan::Octopus)));
    let length = if sprawls { 33.0 } else { 26.0 } * big;
    let tall = 32.0 * big;
    let spacing = length * 1.1;
    let cols = rows[0].len().max(rows[1].len());
    let width = spacing * cols as f32;
    let d = width * 0.55 + 1.0;
    let fwd = Vec3::new(-yaw.sin(), 0.0, -yaw.cos());
    let right = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
    // Birds three-quarters on, so both wings show.
    let face = yaw + std::f32::consts::FRAC_PI_2 + if m.rig.as_ref().is_some_and(|r| r.plan == Some(Plan::Bird)) { 0.7 } else { 0.0 };
    let mut lines = vec![format!("{name} ({mode}), columns left to right:")];
    for (r, row) in rows.iter().enumerate() {
        lines.push(format!(
            "  {} row: {}",
            if r == 0 { "lower" } else { "upper" },
            row.iter().map(|x| x.label.as_str()).collect::<Vec<_>>().join(", ")
        ));
        for (col, c) in row.iter().enumerate() {
            // Floating above eye height, so the sky is the backdrop and no
            // grass hides the feet.
            let p = eye + fwd * d + right * ((col as f32 - (cols as f32 - 1.0) * 0.5) * spacing) + Vec3::Y * (0.3 * tall + r as f32 * tall * 1.15);
            let e = ents.world.spawn((Look::new(c.model, p, face),));
            if let Some(pose) = &c.pose {
                let _ = ents.world.insert_one(e, Anim::hold(pose.clone()));
            }
            if let Some(mo) = &c.morph {
                let _ = ents.world.insert_one(e, mo.clone());
            }
            if let Some(age) = c.age {
                let _ = ents.world.insert_one(e, age);
            }
        }
    }
    Ok(lines)
}

/// The creature, its parent and every relative loaded in this world, at
/// rest (lower row) and mid-stride (upper row).
fn family(ents: &Entities, models: &[Model], si: usize) -> [Vec<Cell>; 2] {
    let sp = &ents.species;
    let parent = |i: usize| sp[i].like.as_ref().and_then(|p| sp.iter().position(|s| &s.name == p));
    let root = |mut i: usize| {
        for _ in 0..8 {
            match parent(i) {
                Some(p) => i = p,
                None => break,
            }
        }
        i
    };
    let r = root(si);
    let members: Vec<usize> = (0..sp.len()).filter(|&i| root(i) == r && !models[sp[i].model].mesh.quads.is_empty()).collect();
    let mut rows: [Vec<Cell>; 2] = [Vec::new(), Vec::new()];
    for &i in &members {
        let m = &models[sp[i].model];
        let label = match (&sp[i].like, sp[i].morphs.is_empty()) {
            (Some(p), _) => format!("{} (like {p})", sp[i].name),
            (None, _) => sp[i].name.clone(),
        };
        let rest = m.rig.as_ref().map(|r| Pose::rest(r.bones.len()));
        rows[0].push(Cell {
            label: label.clone(),
            model: sp[i].model,
            pose: rest,
            morph: None,
            age: None,
        });
        let walk = m.rig.as_ref().map(|rig| {
            let mut l = Life::new(rig.table(sp[i].verb().routine), 3);
            l.gait = motors::gait_for(rig.plan, sp[i].beh.walk_speed, motors::hip(rig) * m.voxel);
            l.gait_mix = 1.0;
            l.gait_w = 1.0;
            l.phase = 0.25;
            l.t = 0.0;
            l.compose(rig, m.voxel)
        });
        rows[1].push(Cell {
            label,
            model: sp[i].model,
            pose: walk,
            morph: None,
            age: None,
        });
    }
    rows
}

/// Twenty individuals of one species, each with its own roll of size, coat
/// and habits; the species' rare morphs close the upper row.
fn herd(ents: &Entities, si: usize) -> [Vec<Cell>; 2] {
    let sp = &ents.species[si];
    let mut rows: [Vec<Cell>; 2] = [Vec::new(), Vec::new()];
    for k in 0..20u32 {
        let mo = Morph::roll(&sp.vary, crate::world::noise::hash32(k.wrapping_mul(0x9e37_79b9) ^ 0xbeef));
        let label = format!("x{:.2}", mo.scale);
        rows[(k / 10) as usize].push(Cell {
            label,
            model: sp.model,
            pose: None,
            morph: Some(mo),
            age: None,
        });
    }
    for &(m, chance) in &sp.morphs {
        let ms = &ents.species[m];
        rows[1].push(Cell {
            label: format!("{} ({:.0}%)", ms.name, chance * 100.0),
            model: ms.model,
            pose: None,
            morph: None,
            age: None,
        });
    }
    rows
}

/// The ages of one creature: the living stages standing (lower row) and
/// mid-stride (upper row), then what is left of it.
fn ages(ents: &Entities, models: &[Model], si: usize) -> [Vec<Cell>; 2] {
    use crate::creatures::age::{Age, Stage};
    let sp = &ents.species[si];
    let m = &models[sp.model];
    let mut rows: [Vec<Cell>; 2] = [Vec::new(), Vec::new()];
    let living = [Stage::Juvenile, Stage::Adult, Stage::Aged];
    for (k, stage) in living.into_iter().enumerate() {
        // Skip a stage this creature doesn't have, so the sheet shows the
        // truth about it rather than a pretend juvenile.
        let has = match stage {
            Stage::Juvenile => sp.ages.has_juvenile(),
            Stage::Aged => sp.ages.aged,
            _ => true,
        };
        if !has {
            continue;
        }
        let mut mo = Morph::roll(&sp.vary, crate::world::noise::hash32(k as u32 + 11));
        mo.age(stage, &sp.ages, sp.aged_tint);
        let stand = m.rig.as_ref().map(|r| Pose::rest(r.bones.len()));
        rows[0].push(Cell {
            label: stage.name().to_string(),
            model: sp.model,
            pose: stand,
            morph: Some(mo.clone()),
            age: None,
        });
        let walk = m.rig.as_ref().map(|rig| {
            let mut l = Life::new(rig.table(sp.verb().routine), 3);
            l.supple = mo.supple;
            l.gait = motors::gait_for(rig.plan, sp.beh.walk_speed, motors::hip(rig) * m.voxel);
            l.gait_mix = 1.0;
            l.gait_w = 1.0;
            l.phase = 0.25;
            l.t = 0.0;
            l.compose(rig, m.voxel)
        });
        rows[1].push(Cell {
            label: format!("{} walking", stage.name()),
            model: sp.model,
            pose: walk,
            morph: Some(mo),
            age: None,
        });
    }
    // Lying on its side, for the creatures that sleep that way.
    if sp.beh.sleeps_on_side {
        if let Some(rig) = m.rig.as_ref() {
            let pose = super::ragdoll::on_side(rig, m.voxel, 1.0, 5);
            rows[1].push(Cell {
                label: "asleep on its side".into(),
                model: sp.model,
                pose: Some(pose),
                morph: None,
                age: None,
            });
        }
    }
    // Three falls of the same skeleton, to show the spread.
    if let Some(rig) = m.skeleton.as_ref() {
        for seed in [1u32, 2, 3] {
            let pose = super::ragdoll::settled(rig, m.voxel, seed);
            let row = if seed == 1 { 0 } else { 1 };
            rows[row].push(Cell {
                label: format!("deceased (fall {seed})"),
                model: sp.model,
                pose: Some(pose),
                morph: None,
                age: Some(Age::old_bones(0.0, 1.0)),
            });
        }
    }
    rows
}

fn bones(rig: &Rig) -> [Vec<(String, Pose)>; 2] {
    let n = rig.bones.len();
    let row = |axis: Vec3| -> Vec<(String, Pose)> {
        (0..n)
            .map(|col| {
                let mut p = Pose::rest(n);
                if col == 0 {
                    return ("rest".to_string(), p);
                }
                p.bones[col] = axis * BEND;
                (rig.bones[col].name.clone(), p)
            })
            .collect()
    };
    [row(Vec3::X), row(Vec3::Y)]
}

fn moves(rig: &Rig, voxel: f32, verb: &Verb, beh: &crate::library::Behaviour) -> [Vec<(String, Pose)>; 2] {
    if rig.plan == Some(Plan::Ray) {
        return ray_moves(rig);
    }
    if rig.plan == Some(Plan::Serpent) || verb.slithers() {
        return serpent_moves(rig, voxel);
    }
    if rig.plan == Some(Plan::Octopus) {
        return crate::creatures::octopus::sheet(rig, voxel, Vec3::new(16.0, 0.0, 16.0));
    }
    if rig.plan == Some(Plan::Bird) && verb.flies() {
        return bird_moves(rig, voxel, verb, beh);
    }
    let base = Life::new(rig.table(verb.routine), 7);
    let with = |f: &dyn Fn(&mut Life)| -> Pose {
        let mut l = base.clone();
        l.t = 0.0;
        l.flick = 0.0;
        f(&mut l);
        l.compose(rig, voxel)
    };
    let mut lower = Vec::new();
    let gaits: &[Gait] = if rig.plan == Some(Plan::Hopper) { &[Gait::Bound] } else { &[Gait::Walk, Gait::Trot, Gait::Gallop] };
    for &g in gaits {
        for k in 0..4 {
            let ph = k as f32 * 0.25;
            lower.push((
                format!("{} {ph}", g.name()),
                with(&|l| {
                    l.gait = g;
                    l.gait_mix = 1.0;
                    l.phase = ph;
                    l.gait_w = 1.0;
                }),
            ));
        }
    }
    let mut upper = vec![
        ("stand".to_string(), with(&|_| {})),
        ("graze".to_string(), with(&|l| l.graze_w = 1.0)),
        ("look left".to_string(), with(&|l| l.look = (1.2, 0.0))),
        (
            "look up, alert".to_string(),
            with(&|l| {
                l.look = (0.0, 0.4);
                l.alert = 1.0;
            }),
        ),
    ];
    if base.table.has_sit {
        upper.push(("sit".to_string(), with(&|l| l.level = motors::SIT)));
        upper.push(("sit to lie".to_string(), with(&|l| l.level = 1.5)));
    } else {
        upper.push(("lying down".to_string(), with(&|l| l.level = 1.0)));
    }
    upper.push((
        "lie".to_string(),
        with(&|l| {
            l.level = motors::LIE;
            l.act = Act::Lie;
        }),
    ));
    upper.push(("alarm".to_string(), with(&|l| l.alarm = 1.0)));
    upper.push(("uphill".to_string(), with(&|l| l.tilt = 0.3)));
    if verb.hides() {
        upper.push(("hiding".to_string(), with(&|l| l.hide = 0.5)));
        upper.push(("in its shell".to_string(), with(&|l| l.hide = 1.0)));
    }
    if rig.plan == Some(Plan::Bird) {
        // A bird that walks (a heron): its flight replaces the postures.
        let [beats, _] = bird_moves(rig, voxel, verb, beh);
        lower.truncate(8);
        return [lower, beats];
    }
    [lower, upper]
}

/// Snakes: a crawl at four phases of its wave below (plus a turn), and the
/// shapes a snake holds above — stretched out, coiled, coiled with its head
/// up and buzzing, up a trunk, and draped along a branch with its head
/// hanging off it. Every one is a path the mover could put it on
/// (`anim/spine.rs`), posed through the same code as the game.
fn serpent_moves(rig: &Rig, voxel: f32) -> [Vec<(String, Pose)>; 2] {
    use crate::anim::spine::{self, Wave};
    let ch = spine::chain(rig);
    let len = ch.length(voxel);
    let pivot = Vec3::new(16.0, 0.0, 16.0);
    // The paths are built about the origin; the sheet draws them wherever the
    // column is, which is what the mover does with the creature's position.
    let along = |pts: &[Vec3]| spine::pose_along(rig, &ch, voxel, pivot, Vec3::ZERO, 0.0, pts);
    let run = |bend: f32| -> Vec<Vec3> {
        let mut pts = vec![Vec3::ZERO];
        for i in 0..ch.n() - 1 {
            let a = bend * i as f32;
            pts.push(pts[i] + Vec3::new(a.sin(), 0.0, a.cos()) * ch.seg(i, voxel));
        }
        pts
    };
    let straight = run(0.0);
    let waved = |pts: &[Vec3], amp: f32, ph: f32| {
        let mut p = along(pts);
        spine::add_wave(&mut p, &ch, voxel, &Wave { amp, lambda: len * 0.6, phase: ph });
        p
    };
    let mut lower: Vec<(String, Pose)> = (0..4)
        .map(|k| {
            let ph = k as f32 * 0.25;
            (format!("crawl {ph}"), waved(&straight, 0.6, ph))
        })
        .collect();
    lower.push(("turning".to_string(), waved(&run(0.12), 0.6, 0.3)));
    lower.push(("swimming".to_string(), waved(&straight, 0.8, 0.15)));
    let trunk: Vec<Vec3> = {
        // Up a trunk in a slow spiral, head at the top.
        let r = len * 0.06 + 0.15;
        let mut pts = Vec::new();
        for i in 0..ch.n() {
            let a = 0.35 * i as f32;
            let s: f32 = (0..i).map(|k| ch.seg(k, voxel)).sum();
            pts.push(Vec3::new(a.sin() * r, -s * 0.93, a.cos() * r));
        }
        pts
    };
    let upper = vec![
        ("stretched out".to_string(), along(&straight)),
        ("coiled".to_string(), along(&spine::coil(&ch, voxel, Vec3::ZERO, 0.0, len * 0.08))),
        ("coiled, head up".to_string(), along(&spine::coil(&ch, voxel, Vec3::ZERO, 0.0, len * 0.16))),
        ("climbing".to_string(), along(&trunk)),
        ("draped".to_string(), along(&spine::drape(&ch, voxel, Vec3::ZERO, Vec3::Z, ch.n() / 3, 0.0))),
        ("draped, swaying".to_string(), along(&spine::drape(&ch, voxel, Vec3::ZERO, Vec3::Z, ch.n() / 3, len * 0.035))),
        ("limp".to_string(), crate::anim::ragdoll::settled(rig, voxel, 5)),
    ];
    [lower, upper]
}

/// Rays: the wing ripple at eight phases below, banking and climbing above.
fn ray_moves(rig: &Rig) -> [Vec<(String, Pose)>; 2] {
    let at = |ph: f32, bank: f32, pitch: f32| {
        let mut p = Pose::rest(rig.bones.len());
        motors::undulate(rig, &mut p, ph, 0.3, 1.0);
        p.bones[0].z += bank;
        p.bones[0].x += pitch;
        p
    };
    let lower = (0..8).map(|k| (format!("ripple {}", k as f32 / 8.0), at(k as f32 / 8.0, 0.0, 0.0))).collect();
    let upper = vec![
        ("rest".to_string(), Pose::rest(rig.bones.len())),
        ("bank left".to_string(), at(0.25, 0.5, 0.0)),
        ("bank right".to_string(), at(0.75, -0.5, 0.0)),
        ("climb".to_string(), at(0.5, 0.0, 0.35)),
        ("dive".to_string(), at(0.0, 0.0, -0.35)),
    ];
    [lower, upper]
}

/// Birds: the wingbeat at eight phases below; glides, folds, banking,
/// landing and perching above.
fn bird_moves(rig: &Rig, voxel: f32, verb: &Verb, beh: &crate::library::Behaviour) -> [Vec<(String, Pose)>; 2] {
    use super::flight::{beat_for, Flight, Pattern};
    // The species' own wingbeat and pattern, as the game picks them.
    let pattern = beh.flight.as_deref().and_then(Pattern::parse).unwrap_or(verb.flight);
    let base = Flight::new(pattern, beh.wingbeat.unwrap_or_else(|| beat_for(rig, voxel)), 5);
    let with = |f: &dyn Fn(&mut Flight)| -> Pose {
        let mut l = base.clone();
        l.t = 0.0;
        l.flapping = 1.0;
        l.look = (0.0, 0.0);
        l.peck = 0.0;
        f(&mut l);
        l.compose(rig, voxel)
    };
    let lower = (0..8)
        .map(|k| {
            let ph = k as f32 / 8.0;
            (format!("beat {ph}"), with(&|l| l.phase = ph))
        })
        .collect();
    let upper = vec![
        ("glide".to_string(), with(&|l| l.flapping = 0.0)),
        (
            "soar".to_string(),
            with(&|l| {
                l.pattern = Pattern::Soar;
                l.flapping = 0.0;
            }),
        ),
        (
            "bound, wings shut".to_string(),
            with(&|l| {
                l.pattern = Pattern::Bound;
                l.resting = true;
                l.flapping = 0.0;
            }),
        ),
        (
            "bank left".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.bank = 0.6;
            }),
        ),
        (
            "climb".to_string(),
            with(&|l| {
                l.pitch = 0.4;
                l.phase = 0.75;
            }),
        ),
        (
            "landing flare".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.flare = 1.0;
                l.pitch = 0.5;
            }),
        ),
        (
            "perched".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
            }),
        ),
        (
            "perched, looking".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
                l.look = (1.0, 0.2);
            }),
        ),
        (
            "pecking".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
                l.peck = 0.5;
            }),
        ),
        (
            "walking, left foot".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
                l.walk = 1.0;
                l.stride = 0.25;
            }),
        ),
        (
            "walking, right foot".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
                l.walk = 1.0;
                l.stride = 0.75;
            }),
        ),
        (
            "clinging, drumming".to_string(),
            with(&|l| {
                l.flapping = 0.0;
                l.fold = 1.0;
                l.pitch = 1.35;
                l.peck = 0.5;
            }),
        ),
    ];
    [lower, upper]
}
