//! Snakes: the `slither`, `climb` and `swim` movements, one mover for the
//! three habits (`Habit` in locomotion.rs).
//!
//! A snake has no legs to swing and no physics body. What it has is a path
//! (`anim/spine.rs`): the mover leads the head and the body follows where the
//! head has been, let down onto whatever it is crossing. That is the whole
//! trick — a snake over a boulder, up a trunk and along a branch are the same
//! code with the head led somewhere else.
//!
//! - ground: crawls, lies coiled, basks stretched out, and a species that
//!   `warns` holds its ground with its head up and its tail buzzing instead
//!   of leaving.
//! - tree: crawls to a trunk, climbs it in a slow spiral, drapes along a
//!   branch with the front of its body hanging off and swaying, and lets go
//!   and falls, limp, when someone comes right underneath.
//! - water: swims at the surface with its head out and a long wave down its
//!   body, ducks under when startled, and hauls out on the bank to lie in
//!   the sun.

use super::age::{Age, Stage};
use super::locomotion::Habit;
use super::plants::Flora;
use super::startle::{self, Nerve, Threat};
use super::terrain::{voxel_ground, water_voxel, Terrain};
use super::{Creature, Look, Pathing, Species};
use crate::anim::spine::{self, Chain, Spine, Wave};
use crate::anim::Anim;
use crate::models::Model;
use crate::world::noise::{hash32, Rng};
use glam::Vec3;
use std::f32::consts::{PI, TAU};

/// How far the body is held off whatever it lies on, metres.
const LIFT: f32 = 0.04;
/// How fast the body is let down onto the ground it crosses, per second.
const SETTLE: f32 = 9.0;
/// How far under the surface a swimming snake carries its backbone, metres:
/// enough that the water parts over its back and only the top of it shows.
const AWASH: f32 = 0.05;
/// How fast a snake can swing its head round, radians a second. A snake turns
/// its whole length through where its head went, so it cannot pivot: without
/// this, a snake that found a cliff in front of it turned about in one step.
const TURN: f32 = 3.0;

/// A tree a snake can climb: trunk and crown.
#[derive(Clone, Copy, Debug)]
pub struct Tree {
    pub base: Vec3,
    pub trunk: f32,
    pub top: f32,
    pub crown: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum Mode {
    /// Still on the ground: coiled, or stretched out if `coiled` is false.
    Rest { coiled: bool },
    /// Crawling over the ground on its heading.
    Crawl,
    /// On a trunk, `h` metres up on the side facing `angle`, bound for `to`
    /// metres where it will lie along a branch.
    Climb { tree: Tree, h: f32, angle: f32, to: f32, up: bool },
    /// Draped along a branch of `tree` at height `h` facing `angle`.
    Drape { tree: Tree, h: f32, angle: f32 },
    /// Let go of the branch: falling, limp.
    Fall { vy: f32 },
    /// At the surface of a river, lake or the sea. `dive` counts down while
    /// it keeps its head under.
    Swim { dive: f32 },
}

pub struct Serpent {
    pub habit: Habit,
    pub spine: Spine,
    pub chain: Chain,
    pub mode: Mode,
    /// The heading it wants.
    pub heading: f32,
    /// The heading it is actually on, which swings round to the wanted one at
    /// `TURN` at most.
    pub face: f32,
    pub timer: f32,
    /// How far the head is through its own side-to-side weave, in cycles.
    pub weave: f32,
    /// 0 stretched out, 1 wound up in a coil.
    pub coiled: f32,
    /// How much of the travelling wave is in the body now: it comes in and
    /// goes out over about a second, since a body cannot snap into a wave.
    pub wave_w: f32,
    /// 0 quiet, 1 the tail buzzing hard.
    pub rattle: f32,
    /// Alarm built up from the player pressing on it (creatures/startle.rs).
    pub alarm: f32,
    /// 0 not bothered, 1 head up and watching the player.
    pub watch: f32,
    /// Seconds in this skin, for sways and buzzes.
    pub t: f32,
    pub rng: Rng,
    /// Which way it last turned aside, so it follows a wall rather than
    /// dithering at it.
    pub side: f32,
}

impl Serpent {
    fn new(habit: Habit, chain: Chain, spine: Spine, mode: Mode, heading: f32, seed: u32) -> Self {
        let mut rng = Rng::walker(seed | 1);
        let side = if seed & 1 == 0 { 1.0 } else { -1.0 };
        Serpent {
            habit,
            spine,
            chain,
            mode,
            heading,
            face: heading,
            timer: 1.0 + rng.unit() * 4.0,
            weave: rng.unit(),
            coiled: 0.0,
            wave_w: 0.0,
            rattle: 0.0,
            alarm: 0.0,
            watch: 0.0,
            t: rng.unit() * 10.0,
            rng,
            side,
        }
    }

    /// Lying still, coiled or stretched out: free for a display.
    pub fn resting(&self) -> bool {
        matches!(self.mode, Mode::Rest { .. })
    }

    pub fn in_tree(&self) -> bool {
        matches!(self.mode, Mode::Climb { .. } | Mode::Drape { .. })
    }

    /// What it is doing, for the headless stats.
    pub fn doing(&self) -> &'static str {
        match self.mode {
            Mode::Rest { coiled: true } => "coiled",
            Mode::Rest { .. } => "basking",
            Mode::Crawl => "crawling",
            Mode::Climb { .. } => "climbing",
            Mode::Drape { .. } => "draped",
            Mode::Fall { .. } => "falling",
            Mode::Swim { .. } => "swimming",
        }
    }
}

/// A tree worth climbing within `rmax` of `at`: tall enough to hold a snake
/// and with a crown to lie in.
fn pick_tree(flora: &Flora, at: Vec3, rmin: f32, rmax: f32, away: Option<Vec3>, rng: &mut Rng) -> Option<Tree> {
    let mut best: Option<(f32, Tree)> = None;
    for p in flora.near(at.x, at.z, rmax, None) {
        if p.trunk < 0.1 || p.height < 3.0 || p.crown < 0.6 {
            continue;
        }
        let d = Vec3::new(p.pos.x - at.x, 0.0, p.pos.z - at.z);
        if d.length() < rmin {
            continue;
        }
        let lean = away.map_or(0.0, |w| d.normalize_or_zero().dot(w) * 2.0);
        let score = lean + rng.unit();
        if best.is_none_or(|b| score > b.0) {
            best = Some((
                score,
                Tree {
                    base: p.pos,
                    trunk: p.trunk,
                    top: p.height,
                    crown: p.crown,
                },
            ));
        }
    }
    best.map(|b| b.1)
}

/// Where the head sits on a trunk `h` metres up, on the side facing `angle`.
fn on_trunk(tree: &Tree, h: f32, angle: f32, body: f32) -> Vec3 {
    let r = tree.trunk * (1.0 - 0.3 * (h / tree.top.max(1.0)).min(1.0)) + 0.02 + body;
    tree.base + Vec3::new(angle.sin() * r, h, angle.cos() * r)
}

/// The branch a draped snake lies on: where the body crosses it, and which
/// way the branch runs back toward the trunk.
fn branch(tree: &Tree, h: f32, angle: f32) -> (Vec3, Vec3) {
    let out = Vec3::new(angle.sin(), 0.0, angle.cos());
    let anchor = tree.base + out * (tree.crown * 0.3).clamp(tree.trunk + 0.1, tree.crown.max(0.3)) + Vec3::Y * h;
    (anchor, -out)
}

/// How high up a snake lies in a tree, for a roll of 0 to 1.
fn drape_height(tree: &Tree, roll: f32) -> f32 {
    tree.top * (0.55 + roll * 0.2)
}

/// The surface of the water at (x, z), if a snake can swim there: a river,
/// lake or the sea with half a metre in it.
fn water_level(terrain: &Terrain, x: f32, z: f32) -> Option<f32> {
    if let Some((level, depth)) = terrain.inland_water(x, z) {
        if depth > 0.4 {
            return Some(level);
        }
    }
    (terrain.sea - terrain.bed(x, z) > 0.4).then_some(terrain.sea)
}

/// Put `n` snakes of species `s` at or near (x, z). False if the spot won't
/// do: no tree for a climber, no water for a swimmer, no open ground.
#[allow(clippy::too_many_arguments)]
pub fn spawn(world: &mut hecs::World, s: usize, sp: &Species, habit: Habit, models: &[Model], terrain: &Terrain, flora: &Flora, at: Vec3, n: u32, seed: u32) -> bool {
    let m = &models[sp.model];
    let Some(rig) = m.rig.as_ref() else { return false };
    let ch = spine::chain(rig);
    if !spine::is_chain(rig) || ch.n() < 4 {
        return false;
    }
    let mut rng = Rng::new(seed | 1);
    let mut placed = 0;
    for k in 0..n.max(1) {
        let a = rng.unit() * TAU;
        let r = if k == 0 { 0.0 } else { 1.5 + rng.unit() * 6.0 };
        let (x, z) = (at.x + a.sin() * r, at.z + a.cos() * r);
        let vox = m.voxel;
        let len = ch.length(vox);
        let heading = rng.unit() * TAU;
        let seed = hash32(rng.state ^ 0x5ba4e ^ k.wrapping_mul(0x9e37));
        let spot = match habit {
            Habit::Water => water_level(terrain, x, z).map(|level| (Vec3::new(x, level - AWASH, z), Mode::Swim { dive: 0.0 })),
            // Climbers are mostly already up a tree when you meet them.
            Habit::Tree if rng.unit() < 0.65 => pick_tree(flora, Vec3::new(x, 0.0, z), 0.0, 18.0, None, &mut rng).map(|tree| {
                let h = drape_height(&tree, rng.unit());
                let angle = rng.unit() * TAU;
                (branch(&tree, h, angle).0, Mode::Drape { tree, h, angle })
            }),
            _ => match voxel_ground(terrain.store, x, z, terrain.store.top_m()) {
                Some((gy, mat)) if gy > terrain.sea + 0.3 && !water_voxel(mat) => Some((Vec3::new(x, gy + LIFT, z), Mode::Rest { coiled: rng.unit() < 0.5 })),
                _ => None,
            },
        };
        let Some((head, mode)) = spot else { continue };
        let stage = super::age::roll(&sp.ages, rng.unit());
        if stage == Stage::Dead {
            continue;
        }
        let mut spine = Spine::straight(&ch, vox, head, heading);
        match mode {
            Mode::Drape { tree, h, angle } => {
                let (anchor, along) = branch(&tree, h, angle);
                spine.pts = spine::drape(&ch, vox, anchor, along, ch.n() / 3, 0.0);
            }
            Mode::Rest { coiled: true } => spine.pts = spine::coil(&ch, vox, head, heading, len * 0.1),
            _ => {}
        }
        let serp = Serpent::new(habit, ch.clone(), spine, mode, heading, seed);
        let pos = serp.spine.head();
        world.spawn((
            serp,
            Creature::of(s),
            Look::new(sp.model, pos, 0.0),
            Age::new(stage),
            Anim::hold(crate::anim::Pose::rest(rig.bones.len())),
        ));
        placed += 1;
    }
    placed > 0
}

/// Step every snake.
#[allow(clippy::too_many_arguments)]
pub fn update(world: &mut hecs::World, species: &[Species], flora: &Flora, models: &[Model], dt: f32, threat: &Threat, terrain: &Terrain, stats: &mut Pathing) {
    for (serp, look, me, morph, anim, age, show) in world.query_mut::<(&mut Serpent, &mut Look, &Creature, Option<&crate::variety::Morph>, &mut Anim, Option<&Age>, Option<&super::display::Show>)>() {
        look.begin_step();
        // A display (creatures/display: two males rising and twining, a
        // millipede curling up) holds it where it is, coiled if the display
        // tucks it, and lays its overlay on the pose made below.
        if let Some(s) = show {
            if !matches!(serp.mode, Mode::Fall { .. }) {
                serp.mode = Mode::Rest { coiled: s.out.pose.tuck > 0.5 };
                serp.timer = serp.timer.max(0.5);
            }
        }
        let sp = &species[me.species];
        let m = &models[sp.model];
        let Some(rig) = m.rig.as_ref() else { continue };
        let scale = morph.map_or(1.0, |x| x.scale);
        let pace = morph.map_or(1.0, |x| x.speed);
        let vox = m.voxel * scale;
        let len = serp.chain.length(vox);
        serp.t += dt;
        serp.timer -= dt;
        stats.snake_steps += 1;
        stats.snakes_in_trees += serp.in_tree() as u32;

        // How hard the player presses on it, and whether that is now enough.
        let nerve = Nerve::of(sp, morph, age.map_or(Stage::Adult, |a| a.stage));
        let facing = Vec3::new(-serp.heading.sin(), 0.0, -serp.heading.cos());
        let press = startle::pressure(&nerve, look.pos, facing, threat, || false);
        let bolts = startle::bolts(&mut serp.alarm, press, dt);
        let k = |rate: f32| (dt * rate).min(1.0);
        // A snake that warns rather than runs: head up and tail buzzing while
        // anything is pressing on it.
        let warns = sp.beh.warns;
        let seen = if press > 0.0 { 1.0 } else { 0.0 };
        serp.watch += (seen - serp.watch) * k(3.0);
        let want_rattle = if warns { seen } else { 0.0 };
        serp.rattle += (want_rattle - serp.rattle) * k(if want_rattle > 0.0 { 6.0 } else { 1.5 });

        let (slow, fast) = (sp.beh.walk_speed * pace, sp.beh.flee_speed * pace);
        let to_player = Vec3::new(threat.pos.x - look.pos.x, 0.0, threat.pos.z - look.pos.z);
        let away = (-to_player).normalize_or_zero();

        let mut speed = 0.0;
        // Matched on a copy of the mode, so an arm can set the next one.
        match serp.mode {
            Mode::Rest { coiled } => {
                if bolts && !warns {
                    serp.mode = Mode::Crawl;
                    serp.heading = away.x.atan2(away.z) + serp.rng.signed() * 0.4;
                    serp.timer = 2.0 + serp.rng.unit() * 2.0;
                } else if serp.timer <= 0.0 {
                    if serp.rng.unit() < 0.55 {
                        serp.mode = Mode::Crawl;
                        serp.heading += serp.rng.signed() * 1.6;
                        serp.timer = 3.0 + serp.rng.unit() * 6.0;
                    } else {
                        serp.mode = Mode::Rest { coiled: serp.rng.unit() < 0.55 };
                        serp.timer = 4.0 + serp.rng.unit() * 14.0;
                    }
                }
                let want = if coiled || serp.rattle > 0.3 { 1.0 } else { 0.0 };
                serp.coiled += (want - serp.coiled) * k(1.6);
            }
            Mode::Crawl => {
                speed = if serp.alarm > 0.0 || bolts { fast } else { slow };
                if bolts {
                    serp.heading = away.x.atan2(away.z) + serp.rng.signed() * 0.4;
                    serp.timer = serp.timer.max(2.5);
                }
                serp.coiled += (0.0 - serp.coiled) * k(3.0);
                if serp.timer <= 0.0 {
                    let tree = if serp.habit == Habit::Tree {
                        pick_tree(flora, look.pos, 0.0, 16.0, None, &mut serp.rng)
                    } else {
                        None
                    };
                    match (tree, serp.habit) {
                        // A climber makes for the foot of a trunk and goes up.
                        (Some(tree), _) => {
                            let foot = Vec3::new(tree.base.x - look.pos.x, 0.0, tree.base.z - look.pos.z);
                            if foot.length() < tree.trunk + 0.8 {
                                let angle = foot.x.atan2(foot.z);
                                let to = drape_height(&tree, serp.rng.unit());
                                serp.mode = Mode::Climb { tree, h: 0.0, angle, to, up: true };
                                serp.timer = 40.0;
                            } else {
                                serp.heading = foot.x.atan2(foot.z);
                                serp.timer = 2.0 + serp.rng.unit() * 3.0;
                            }
                        }
                        // A swimmer out on the bank goes back in.
                        (None, Habit::Water) => {
                            serp.timer = 4.0 + serp.rng.unit() * 4.0;
                            match water_level(terrain, look.pos.x, look.pos.z) {
                                Some(level) => {
                                    serp.mode = Mode::Swim { dive: 0.0 };
                                    serp.spine.pts[0].y = level - AWASH;
                                }
                                None => serp.heading = toward_water(terrain, look.pos, serp.heading),
                            }
                        }
                        (None, _) => {
                            serp.mode = Mode::Rest { coiled: serp.rng.unit() < 0.6 };
                            serp.timer = 4.0 + serp.rng.unit() * 20.0;
                        }
                    }
                }
            }
            Mode::Climb { tree, mut h, mut angle, to, up } => {
                let climb = slow * 0.7;
                h += if up { climb * dt } else { -climb * dt * 1.4 };
                angle += serp.side * 0.5 * dt;
                if up && h >= to {
                    serp.mode = Mode::Drape { tree, h, angle };
                    serp.timer = 20.0 + serp.rng.unit() * 60.0;
                } else if !up && h <= 0.05 {
                    serp.mode = Mode::Crawl;
                    serp.timer = 4.0 + serp.rng.unit() * 6.0;
                } else {
                    serp.mode = Mode::Climb { tree, h, angle, to, up };
                }
                let head = on_trunk(&tree, h.max(0.0), angle, len * 0.04);
                serp.spine.lead(&serp.chain, vox, head);
                serp.spine.phase -= climb * dt / (len * 0.6);
                look.pos = head;
            }
            Mode::Drape { tree, h, angle } => {
                // Lying along the branch, swaying, with its head hanging off.
                let (anchor, along) = branch(&tree, h, angle);
                let sway = (serp.t * 0.55).sin() * len * 0.035;
                let path = spine::drape(&serp.chain, vox, anchor, along, serp.chain.n() / 3, sway);
                serp.spine.ease(&path, k(2.5));
                serp.spine.tighten(&serp.chain, vox);
                look.pos = serp.spine.head();
                // Someone right underneath: it lets go and drops.
                let close = startle::reach(&nerve, threat, look.pos, 2.5);
                if (bolts || to_player.length() < close * 0.4) && h > 0.8 {
                    serp.mode = Mode::Fall { vy: 0.0 };
                    serp.heading = away.x.atan2(away.z);
                    // Limp, held by the head where the fall has got to:
                    // the frame is the head (`look.pos`), so it is held
                    // at the frame's origin all the way down.
                    let limp = anim.go_limp(rig, vox, m.pivot);
                    limp.hold_bone(serp.chain.bones[0], Vec3::ZERO);
                    serp.timer = 4.0;
                } else if serp.timer <= 0.0 {
                    serp.mode = Mode::Climb { tree, h, angle, to: h, up: false };
                    serp.timer = 40.0;
                }
            }
            Mode::Fall { vy } => {
                let vy = vy - 24.0 * dt;
                let mut head = serp.spine.head() + Vec3::Y * vy * dt;
                match voxel_ground(terrain.store, head.x, head.z, head.y + 1.0) {
                    Some((g, _)) if head.y <= g + LIFT => {
                        head.y = g + LIFT;
                        serp.spine = Spine::straight(&serp.chain, vox, head, serp.heading);
                        serp.mode = Mode::Crawl;
                        serp.timer = 3.0 + serp.rng.unit() * 4.0;
                        anim.limp = None;
                    }
                    _ => {
                        // Limp all the way down: the body trails from the head.
                        let shift = head - serp.spine.head();
                        for p in &mut serp.spine.pts {
                            *p += shift;
                        }
                        serp.mode = Mode::Fall { vy };
                        // The head falls faster than anything let go
                        // would, so in its frame the body is pushed up
                        // and trails above it, with a little sideways.
                        anim.step_limp(rig, dt, &|_| -1.0e4, Vec3::new(serp.side * 2.0, 24.0, 0.0));
                    }
                }
                look.pos = serp.spine.head();
            }
            Mode::Swim { dive } => {
                speed = if serp.alarm > 0.0 || bolts { fast } else { slow } * 0.9;
                let mut dive = (dive - dt).max(0.0);
                if bolts {
                    serp.heading = away.x.atan2(away.z) + serp.rng.signed() * 0.3;
                    dive = 1.5 + serp.rng.unit() * 2.0;
                    serp.timer = serp.timer.max(3.0);
                }
                if serp.timer <= 0.0 {
                    serp.timer = 6.0 + serp.rng.unit() * 10.0;
                    serp.heading += serp.rng.signed() * 1.2;
                    // Now and then out onto the bank to lie in the sun.
                    if serp.rng.unit() < 0.2 {
                        serp.mode = Mode::Crawl;
                        serp.heading = away_from_water(terrain, look.pos, serp.heading);
                        serp.timer = 5.0 + serp.rng.unit() * 5.0;
                        serp.coiled = 0.0;
                    }
                }
                if let Mode::Swim { .. } = serp.mode {
                    serp.mode = Mode::Swim { dive };
                }
            }
        }

        // Crawling and swimming: lead the head, weaving as it goes.
        let in_water = matches!(serp.mode, Mode::Swim { .. });
        if speed > 0.0 {
            let wavelen = len * 0.6;
            serp.weave += speed * dt / wavelen;
            let weave = (TAU * serp.weave).sin() * if in_water { 0.45 } else { 0.3 };
            if !in_water {
                serp.heading = clear_heading(terrain, look.pos, serp.face, speed, &mut serp.side);
            }
            let turn = crate::util::wrap_angle(serp.heading - serp.face);
            serp.face += turn.clamp(-TURN * dt, TURN * dt);
            let a = serp.face + weave;
            let mut head = serp.spine.head() + Vec3::new(a.sin(), 0.0, a.cos()) * speed * dt;
            match (in_water, water_level(terrain, head.x, head.z)) {
                (true, Some(level)) => {
                    let dive = if let Mode::Swim { dive } = serp.mode { dive } else { 0.0 };
                    let current = terrain.current(head.x, head.z);
                    head.x += current.x * dt;
                    head.z += current.y * dt;
                    head.y = level - AWASH - if dive > 0.0 { len * 0.08 } else { 0.0 };
                }
                // Out of water: turn back toward it and stay put this step.
                (true, None) => {
                    serp.heading = toward_water(terrain, look.pos, serp.heading + PI);
                    head = serp.spine.head();
                }
                _ => {
                    if let Some((g, _)) = voxel_ground(terrain.store, head.x, head.z, head.y + 0.6) {
                        head.y = g + LIFT;
                    }
                }
            }
            serp.spine.lead(&serp.chain, vox, head);
            // The wave stands still on the ground while the body slides
            // through it, which is what lateral undulation is.
            serp.spine.phase -= speed * dt / wavelen;
            look.pos = serp.spine.head();
            stats.moving += 1;
        } else if matches!(serp.mode, Mode::Rest { .. }) {
            // Winding up into a coil, or lying out along the ground.
            if serp.coiled > 0.01 {
                let centre = serp.spine.pts[serp.chain.n() / 2];
                let yaw = if serp.watch > 0.3 { to_player.x.atan2(to_player.z) } else { serp.heading };
                let rise = len * (0.08 + 0.06 * serp.rattle);
                let mut target = spine::coil(&serp.chain, vox, centre, yaw, rise);
                if serp.coiled < 0.99 {
                    for (t, p) in target.iter_mut().zip(&serp.spine.pts) {
                        *t = p.lerp(*t, serp.coiled);
                    }
                }
                serp.spine.ease(&target, k(3.0));
                serp.spine.tighten(&serp.chain, vox);
            }
            look.pos = serp.spine.head();
        }

        // The body let down onto the ground it lies over, which is what puts
        // a snake across a rock rather than through it.
        if !in_water && !serp.in_tree() && !matches!(serp.mode, Mode::Fall { .. }) {
            let from = look.pos.y + 1.2;
            serp.spine
                .settle(&|x, z| voxel_ground(terrain.store, x, z, from).map(|g| g.0), LIFT, k(if speed > 0.0 { SETTLE } else { 4.0 }));
            serp.spine.tighten(&serp.chain, vox);
        } else if in_water {
            let level = water_level(terrain, look.pos.x, look.pos.z).unwrap_or(look.pos.y);
            for p in &mut serp.spine.pts {
                p.y += (level - AWASH - p.y) * k(6.0);
            }
            serp.spine.tighten(&serp.chain, vox);
        }
        // The tail buzz: a fast, small shake of the last joint.
        if serp.rattle > 0.02 {
            let n = serp.spine.pts.len();
            let d = serp.spine.pts[n - 1] - serp.spine.pts[n - 2];
            let perp = Vec3::new(d.z, 0.0, -d.x).normalize_or_zero();
            serp.spine.pts[n - 1] += perp * (serp.t * 70.0).sin() * len * 0.02 * serp.rattle + Vec3::Y * len * 0.05 * serp.rattle;
        }

        // The pose: the body drawn along its path. Every heading is in the
        // path, so the drawn yaw stays 0 and nothing fights the interpolation
        // between fixed steps.
        serp.wave_w += ((speed > 0.0) as u32 as f32 - serp.wave_w) * k(2.5);
        if !matches!(serp.mode, Mode::Fall { .. }) {
            let amp = if in_water { 0.8 } else { 0.6 };
            let w = Wave {
                amp: amp * serp.wave_w * (1.0 - serp.coiled),
                lambda: len * 0.6,
                phase: serp.spine.phase,
            };
            look.yaw = 0.0;
            let mut pose = serp.spine.pose(rig, &serp.chain, vox, m.pivot, look.pos, 0.0, &w);
            spine::continue_from(&mut pose, &anim.pose);
            spine::ease_joints(&mut pose, &anim.pose, dt);
            anim.prev = std::mem::replace(&mut anim.pose, pose);
            if anim.prev.bones.len() != anim.pose.bones.len() {
                anim.prev = anim.pose.clone();
            }
            if let Some(s) = show {
                let o = super::display::Overlay { tuck: 0.0, ..s.out.pose };
                super::display::apply(rig, &mut anim.pose, &o, vox);
            }
        }
    }
}

/// A heading that doesn't take the snake into a wall, a drop or water: the
/// one it has if that is clear, else the nearest clear one, trying the side
/// it last turned first.
fn clear_heading(terrain: &Terrain, at: Vec3, heading: f32, speed: f32, side: &mut f32) -> f32 {
    let reach = (0.6 + speed * 0.6).clamp(0.8, 2.0);
    let clear = |a: f32| {
        let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * reach;
        if p.x < 4.0 || p.z < 4.0 || p.x > terrain.size() - 4.0 || p.z > terrain.size() - 4.0 {
            return false;
        }
        match voxel_ground(terrain.store, p.x, p.z, at.y + 1.0) {
            // A snake goes over a kerb but not up a wall or off a ledge.
            Some((g, mat)) => !water_voxel(mat) && (g - at.y).abs() < 0.6 && g > terrain.sea + 0.1,
            None => false,
        }
    };
    if clear(heading) {
        return heading;
    }
    for step in 1..=7 {
        for s in [*side, -*side] {
            let a = heading + s * step as f32 * 0.45;
            if clear(a) {
                *side = s;
                return a;
            }
        }
    }
    heading + PI
}

/// The way to the nearest water, by sampling round the snake.
fn toward_water(terrain: &Terrain, at: Vec3, heading: f32) -> f32 {
    let mut best = (terrain.shore_dist(at.x, at.z).unwrap_or(f32::MAX), heading);
    for k in 0..8 {
        let a = k as f32 * TAU / 8.0;
        let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * 3.0;
        let d = terrain.shore_dist(p.x, p.z).unwrap_or(f32::MAX);
        if d < best.0 {
            best = (d, a);
        }
    }
    best.1
}

/// The way out of the water onto the nearest dry bank it can get up.
fn away_from_water(terrain: &Terrain, at: Vec3, heading: f32) -> f32 {
    let mut best: Option<(f32, f32)> = None;
    for k in 0..8 {
        let a = k as f32 * TAU / 8.0;
        let p = at + Vec3::new(a.sin(), 0.0, a.cos()) * 4.0;
        let Some((g, mat)) = voxel_ground(terrain.store, p.x, p.z, at.y + 2.0) else {
            continue;
        };
        let rise = g - at.y;
        if water_voxel(mat) || g < terrain.sea + 0.1 || rise > 1.2 {
            continue;
        }
        if best.is_none_or(|b| rise < b.0) {
            best = Some((rise, a));
        }
    }
    best.map_or(heading, |b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::locomotion;
    use crate::world::store::Store;

    /// Flat solid ground with its surface at `GROUND`, a hundred metres
    /// across. `Store::set` only writes into a chunk that is already there,
    /// so the ground is whole uniform chunks (as the streamer inserts them).
    const GROUND: f32 = 8.0;

    fn flat() -> Store {
        let mut store = Store::default();
        for cx in -1..13 {
            for cz in -1..13 {
                store.insert_column(cx, cz, 3, 1, Vec::new());
            }
        }
        store
    }

    fn species(name: &str, movement: &str, warns: bool) -> Species {
        let beh: crate::library::Behaviour =
            serde_json::from_value(serde_json::json!({ "movement": movement, "walk_speed": 0.6, "flee_speed": 2.0, "wary": 4.0, "flightiness": 1.0, "warns": warns, "flees_from": ["player"] }))
                .unwrap();
        Species {
            name: name.into(),
            model: 0,
            beh,
            flees: vec![],
            hunts: vec![],
            flees_player: true,
            like: None,
            quota: 0,
            water: None,
            is_water_form: false,
            vary: Default::default(),
            coats: vec![],
            morphs: vec![],
            ages: Default::default(),
            aged_tint: [1.0; 3],
            forms: Default::default(),
        }
    }

    /// A library snake, sliced the way worldload does it.
    fn snake_model(name: &str) -> Model {
        let (lib, errors) = crate::library::Library::load(&crate::library::data_root());
        assert!(errors.is_empty(), "{errors:?}");
        let c = lib.creatures.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("no {name} in the library"));
        let names: Vec<&String> = c.colors.keys().collect();
        let resolve = |n: &str| {
            if n == "air" {
                Some(0u8)
            } else {
                names.iter().position(|x| x.as_str() == n).map(|k| k as u8 + 1)
            }
        };
        let whole = crate::world::shape::compile(&c.ops, c.grid, &resolve).mat;
        let s = crate::anim::slice::slice(c.rig.as_ref().expect("rig"), &c.ops, &whole, crate::anim::slice::Dims(c.grid), &|n| resolve(n).is_some(), c.voxel, c.half_w, c.height);
        let mut m = crate::models::from_cells(&whole, c.voxel, c.half_w, c.height);
        m.rig = Some(s.rig);
        m
    }

    fn put(world: &mut hecs::World, model: &Model, mode: Mode, head: Vec3, seed: u32) -> (hecs::Entity, Chain) {
        let rig = model.rig.as_ref().unwrap();
        let ch = spine::chain(rig);
        let spine = Spine::straight(&ch, model.voxel, head, 0.0);
        let e = world.spawn((
            Serpent::new(Habit::Ground, ch.clone(), spine, mode, 0.0, seed),
            Creature::of(0),
            Look::new(0, head, 0.0),
            Anim::hold(crate::anim::Pose::rest(rig.bones.len())),
        ));
        (e, ch)
    }

    /// Five minutes of a rattlesnake on flat ground: it crawls and it rests,
    /// its body stays in one piece and on the ground, and no joint jumps.
    #[test]
    fn a_rattlesnake_crawls_and_rests_without_coming_apart() {
        let store = flat();
        let models = vec![snake_model("rattlesnake")];
        let sp = species("rattlesnake", "slither", true);
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let mut world = hecs::World::new();
        let (e, ch) = put(&mut world, &models[0], Mode::Crawl, Vec3::new(50.0, GROUND, 50.0), 7);
        let threat = Threat::new(Vec3::new(200.0, GROUND, 200.0), Vec3::ZERO, crate::ride::Mode::Foot);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        let (mut moved, mut worst) = (0u32, 0.0f32);
        let mut where_worst = String::new();
        let mut prev: Option<crate::anim::Pose> = None;
        let steps = 300 * 30;
        for step in 0..steps {
            update(&mut world, std::slice::from_ref(&sp), &Flora::default(), &models, dt, &threat, &terrain, &mut stats);
            let serp = world.get::<&Serpent>(e).unwrap();
            for i in 1..serp.spine.pts.len() {
                let d = (serp.spine.pts[i] - serp.spine.pts[i - 1]).length();
                assert!((d - ch.seg(i - 1, models[0].voxel)).abs() < 1e-3, "the body came apart: segment {i} is {d} m");
            }
            // On the ground: every point within a head height of the
            // surface under it (a raised head and a coil are the only lift).
            for (i, p) in serp.spine.pts.iter().enumerate() {
                let (g, _) = voxel_ground(&store, p.x, p.z, p.y + 1.0).unwrap_or_else(|| panic!("point {i} left the test ground at {p:?}"));
                assert!((p.y - g).abs() < 0.25, "point {i} is {:.2} m off the ground at step {step}", p.y - g);
            }
            moved += !matches!(serp.mode, Mode::Rest { .. }) as u32;
            let anim = world.get::<&Anim>(e).unwrap();
            if let Some(prev) = prev.as_ref() {
                for (i, (a, b)) in anim.pose.bones.iter().zip(&prev.bones).enumerate() {
                    let d = (*a - *b).abs().max_element();
                    if d > worst {
                        worst = d;
                        where_worst = format!("bone {i} at step {step}, {}", serp.doing());
                    }
                }
            }
            prev = Some(anim.pose.clone());
        }
        assert!(moved > steps / 20, "it hardly moved: {moved} of {steps} steps");
        assert!(moved < steps * 9 / 10, "it never rested: {moved} of {steps} steps");
        // Nothing turns faster than a body can (anim/spine.rs JOINT_RATE).
        assert!(worst <= crate::anim::spine::JOINT_RATE * dt + 1e-3, "a joint jumped {worst:.3} rad in one step: {where_worst}");
    }

    /// Standing over a rattlesnake makes it coil and buzz rather than leave.
    #[test]
    fn a_rattlesnake_stands_its_ground_and_buzzes() {
        let store = flat();
        let models = vec![snake_model("rattlesnake")];
        let sp = species("rattlesnake", "slither", true);
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let mut world = hecs::World::new();
        let head = Vec3::new(20.0, GROUND, 20.0);
        let (e, _) = put(&mut world, &models[0], Mode::Rest { coiled: false }, head, 3);
        let threat = Threat::new(head + Vec3::new(1.2, 0.0, 0.0), Vec3::new(-0.5, 0.0, 0.0), crate::ride::Mode::Foot);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        for _ in 0..120 {
            update(&mut world, std::slice::from_ref(&sp), &Flora::default(), &models, dt, &threat, &terrain, &mut stats);
        }
        let serp = world.get::<&Serpent>(e).unwrap();
        assert!(serp.rattle > 0.5, "it is not buzzing: {}", serp.rattle);
        assert!(serp.coiled > 0.5, "it did not coil: {}", serp.coiled);
        assert!(matches!(serp.mode, Mode::Rest { .. }), "it left instead of warning");
    }

    /// A snake that does not warn leaves instead.
    #[test]
    fn a_quiet_snake_crawls_away_from_you() {
        let store = flat();
        let models = vec![snake_model("rattlesnake")];
        let sp = species("gopher_snake", "slither", false);
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let mut world = hecs::World::new();
        let head = Vec3::new(20.0, GROUND, 20.0);
        let (e, _) = put(&mut world, &models[0], Mode::Rest { coiled: true }, head, 11);
        let threat = Threat::new(head + Vec3::new(1.5, 0.0, 0.0), Vec3::new(-1.0, 0.0, 0.0), crate::ride::Mode::Foot);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        let mut left = false;
        for _ in 0..300 {
            update(&mut world, std::slice::from_ref(&sp), &Flora::default(), &models, dt, &threat, &terrain, &mut stats);
            left |= !matches!(world.get::<&Serpent>(e).unwrap().mode, Mode::Rest { .. });
        }
        assert!(left, "it sat there with someone standing over it");
        assert!(world.get::<&Serpent>(e).unwrap().rattle < 0.05, "it buzzed without a rattle");
    }

    /// A snake up a tree with someone right underneath lets go, falls limp
    /// (anim/limp.rs), lands on the ground and crawls off.
    #[test]
    fn a_tree_snake_drops_when_you_stand_under_it() {
        let store = flat();
        let models = vec![snake_model("rattlesnake")];
        let sp = species("gopher_snake", "climb", false);
        let terrain = Terrain {
            store: &store,
            land: None,
            sea: -100.0,
        };
        let mut world = hecs::World::new();
        let tree = Tree {
            base: Vec3::new(50.0, GROUND, 50.0),
            trunk: 0.2,
            top: 6.0,
            crown: 1.6,
        };
        let (anchor, _) = branch(&tree, 3.5, 0.0);
        let (e, _) = put(&mut world, &models[0], Mode::Drape { tree, h: 3.5, angle: 0.0 }, anchor, 5);
        let threat = Threat::new(anchor.with_y(GROUND), Vec3::new(0.0, 0.0, -0.4), crate::ride::Mode::Foot);
        let dt = 1.0 / 30.0;
        let mut stats = Pathing::default();
        let (mut fell, mut limp) = (false, false);
        for _ in 0..600 {
            update(&mut world, std::slice::from_ref(&sp), &Flora::default(), &models, dt, &threat, &terrain, &mut stats);
            let serp = world.get::<&Serpent>(e).unwrap();
            if matches!(serp.mode, Mode::Fall { .. }) {
                fell = true;
                limp |= world.get::<&Anim>(e).unwrap().limp.is_some();
            }
        }
        assert!(fell, "it stayed on the branch with someone underneath");
        assert!(limp, "it fell without going limp");
        let serp = world.get::<&Serpent>(e).unwrap();
        assert!(matches!(serp.mode, Mode::Crawl | Mode::Rest { .. }), "it never landed: {:?}", serp.mode);
        assert!((serp.spine.head().y - GROUND).abs() < 0.3, "it landed {:.2} m off the ground", serp.spine.head().y - GROUND);
        assert!(world.get::<&Anim>(e).unwrap().limp.is_none(), "it is still limp after landing");
    }

    #[test]
    fn the_verbs_name_this_mover() {
        for (name, habit) in [("slither", Habit::Ground), ("climb", Habit::Tree), ("swim", Habit::Water)] {
            assert_eq!(locomotion::verb(name).mover, locomotion::Mover::Slither { habit }, "{name}");
        }
    }
}
