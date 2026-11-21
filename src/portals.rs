//! Places that do something to whoever steps into them: a teleporter that
//! puts you somewhere else facing a new way, and a pad that throws you.
//!
//! Both are the same shape of thing -- a standing cylinder that acts on a
//! body whose feet are inside it -- so they are one list in `world.json`
//! rather than two:
//!
//! ```json
//! "portals": [
//!   {"name": "pit", "at": [110.0, 92.0], "up": 0.0, "to": "gallery", "both": true},
//!   {"name": "gallery", "at": [148.0, 60.0], "up": 11.0, "bearing": 180},
//!   {"name": "kick", "at": [96.0, 40.0], "up": 0.0, "push": [0, 6.7, 0]}
//! ]
//! ```
//!
//! `at` and `up` place the foot of the cylinder, `up` measured from the floor
//! the world's own spawn point stands on, as nav nodes and pickup pads are.
//! `to` names the portal a body is sent to. `push` is metres a second: the
//! vertical part replaces what the body had (that is what makes a jump pad a
//! jump pad) and the level part is added to it.
//!
//! `bearing` is a portal's exit direction: the way a body faces when it
//! comes out of this one, degrees clockwise from north. Its level speed is
//! pointed the same way, so a teleporter you run into at full tilt puts you
//! out running. A portal with no `bearing` faces arrivals the most open way
//! out, measured from the voxels round it when it is placed, so nobody lands
//! nose to a wall.
//!
//! A portal that is only a destination needs no `to`. `both` on the sending
//! end makes the pair two-way: the far end sends back. Bots know a way only
//! from a nav link; the arena importer writes one each way for a two-way
//! pair, and a world edited by hand needs its return link added too.
//!
//! A portal acts on a body as it steps in, not while it stands there, so
//! arriving in a two-way pad does not send you straight back: you have to
//! step off and on again.

use crate::player::Player;
use glam::Vec3;
use serde::Deserialize;

/// How far above the foot of a portal a body still counts as in it.
const REACH_UP: f32 = 2.0;
/// And below, so a body dropping into one is caught.
const REACH_DOWN: f32 = 0.5;
/// How far out the open way from a portal is measured, metres. Past this a
/// direction counts as open as any other.
const LOOK_M: f32 = 8.0;
/// How many directions are tried for the open way out.
const LOOKS: usize = 16;

#[derive(Deserialize, Clone, Debug)]
pub struct PortalDef {
    pub name: String,
    /// Metres, `[x, z]`.
    pub at: [f32; 2],
    /// Metres above the floor the world's own spawn point stands on.
    #[serde(default)]
    pub up: f32,
    /// How wide it reaches, metres.
    #[serde(default = "default_radius")]
    pub radius: f32,
    /// The portal a body entering this one is sent to, by name.
    #[serde(default)]
    pub to: Option<String>,
    /// The far end sends back here.
    #[serde(default)]
    pub both: bool,
    /// The exit direction: the way a body faces coming out of this portal,
    /// degrees clockwise from north. None for the most open way out.
    #[serde(default)]
    pub bearing: Option<f32>,
    /// Metres a second: a pad that throws whoever steps on it.
    #[serde(default)]
    pub push: Option<[f32; 3]>,
}

fn default_radius() -> f32 {
    1.0
}

pub struct Portal {
    pub name: String,
    at: Vec3,
    radius: f32,
    to: Option<usize>,
    yaw: Option<f32>,
    push: Option<Vec3>,
    /// How many bodies it has sent or thrown, for the headless report.
    pub used: u32,
}

impl Portal {
    fn holds(&self, p: Vec3) -> bool {
        p.y < self.at.y + REACH_UP && p.y > self.at.y - REACH_DOWN && (p.x - self.at.x).powi(2) + (p.z - self.at.z).powi(2) < self.radius * self.radius
    }
}

/// What a portal does to the body that stepped into it.
pub enum Step {
    /// Go there, facing that way, with your level speed pointed along it.
    Go { to: Vec3, yaw: Option<f32> },
    /// Be thrown: the level part is added, the vertical part replaces.
    Push(Vec3),
}

#[derive(Default)]
pub struct Portals {
    pub list: Vec<Portal>,
    defs: Vec<PortalDef>,
    placed: bool,
}

/// Which way a yaw looks, level.
fn ahead(yaw: f32) -> Vec3 {
    Vec3::new(-yaw.sin(), 0.0, -yaw.cos())
}

impl Portals {
    pub fn new(defs: &[PortalDef], mut warn: impl FnMut(String)) -> Portals {
        let mut list = Vec::new();
        for d in defs {
            let to = match &d.to {
                None => None,
                Some(name) => match defs.iter().position(|o| &o.name == name) {
                    Some(i) => Some(i),
                    None => {
                        warn(format!("portal {} is sent to {name}, which is not a portal in this world", d.name));
                        None
                    }
                },
            };
            list.push(Portal {
                name: d.name.clone(),
                at: Vec3::ZERO,
                radius: d.radius.max(0.25),
                to,
                yaw: d.bearing.map(|b| -b.to_radians()),
                push: d.push.map(Vec3::from),
                used: 0,
            });
        }
        // Two-way pairs: the far end sends back, unless it already sends
        // somewhere of its own.
        for (i, d) in defs.iter().enumerate() {
            let Some(j) = list[i].to.filter(|_| d.both) else { continue };
            match list[j].to {
                None if list[j].push.is_none() => list[j].to = Some(i),
                Some(k) if k == i => {}
                _ => warn(format!("portal {} is two-way, but {} already does something else", d.name, list[j].name)),
            }
        }
        Portals {
            list,
            defs: defs.to_vec(),
            placed: false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Put every portal on the floor under where the world said it is, once
    /// the floor the world measures its heights from is known. `floor` is
    /// the same question the shooter asks of its nav nodes; `open` is how
    /// many metres are clear from a point along a level direction, up to
    /// the distance asked, which gives a portal with no stated exit
    /// direction its most open way out.
    pub fn settle(&mut self, base: f32, floor: impl Fn(f32, f32, f32) -> Option<f32>, open: impl Fn(Vec3, Vec3, f32) -> f32) {
        if self.placed || self.list.is_empty() {
            return;
        }
        self.placed = true;
        for (p, d) in self.list.iter_mut().zip(&self.defs) {
            let want = base + d.up;
            let y = floor(d.at[0], d.at[1], want + 1.0).unwrap_or(want);
            p.at = Vec3::new(d.at[0], y, d.at[1]);
            if p.yaw.is_none() && p.push.is_none() {
                p.yaw = Some(open_way(p.at, &open));
            }
        }
    }

    /// What should happen to a body whose feet went from `was` to `p`, if
    /// anything: only a body stepping in is acted on.
    pub fn enter(&mut self, was: Vec3, p: Vec3) -> Option<Step> {
        if !self.placed {
            return None;
        }
        let i = self.list.iter().position(|q| (q.to.is_some() || q.push.is_some()) && q.holds(p) && !q.holds(was))?;
        self.list[i].used += 1;
        if let Some(j) = self.list[i].to {
            return Some(Step::Go {
                to: self.list[j].at,
                yaw: self.list[j].yaw,
            });
        }
        self.list[i].push.map(Step::Push)
    }

    /// Act on one body. Separate from `enter` so the rules for moving a body
    /// are written once, for the player and for every bot alike.
    pub fn carry(&mut self, b: &mut Player) {
        let Some(step) = self.enter(b.prev_pos, b.pos) else { return };
        match step {
            Step::Go { to, yaw } => {
                b.pos = to + Vec3::Y * 0.05;
                // Drawn from where it was, so it has to come along or the
                // body is drawn streaking across the map. It also marks the
                // body as already standing in the far end, so a two-way pad
                // does not send it straight back.
                b.prev_pos = b.pos;
                // Off the floor it was standing on, until the next step finds
                // the one it has landed on.
                b.body.on_ground = false;
                if let Some(y) = yaw {
                    b.yaw = y;
                    let v = b.body.vel;
                    let level = Vec3::new(v.x, 0.0, v.z).length();
                    b.body.vel = ahead(y) * level + Vec3::Y * v.y;
                }
            }
            Step::Push(by) => {
                b.body.vel.x += by.x;
                b.body.vel.z += by.z;
                if by.y != 0.0 {
                    b.body.vel.y = by.y;
                }
                b.body.on_ground = false;
            }
        }
    }

    /// Every portal and how often it has been used: what the headless report
    /// prints, since a teleporter nobody can reach looks like one that works.
    pub fn report(&self) -> serde_json::Value {
        let rows: Vec<serde_json::Value> = self
            .list
            .iter()
            .map(|p| {
                serde_json::json!({
                    "name": p.name,
                    "at": [(p.at.x * 10.0).round() / 10.0, (p.at.y * 10.0).round() / 10.0, (p.at.z * 10.0).round() / 10.0],
                    "kind": if p.to.is_some() { "teleport" } else if p.push.is_some() { "push" } else { "destination" },
                    "bearing": p.yaw.map(|y| ((-y.to_degrees()).rem_euclid(360.0)).round()),
                    "used": p.used,
                })
            })
            .collect();
        serde_json::Value::Array(rows)
    }
}

/// The yaw that looks the most open way out from a portal's foot: each of
/// `LOOKS` directions scores the clear distance at knee and head height,
/// taken with its two neighbours so a slit between pillars does not win.
fn open_way(at: Vec3, open: &impl Fn(Vec3, Vec3, f32) -> f32) -> f32 {
    let clear: Vec<f32> = (0..LOOKS)
        .map(|k| {
            let d = ahead(-(k as f32) * std::f32::consts::TAU / LOOKS as f32);
            open(at + Vec3::Y * 0.5, d, LOOK_M).min(open(at + Vec3::Y * 1.6, d, LOOK_M))
        })
        .collect();
    let score = |k: usize| clear[k] + 0.5 * (clear[(k + 1) % LOOKS] + clear[(k + LOOKS - 1) % LOOKS]);
    let best = (0..LOOKS).max_by(|&a, &b| score(a).total_cmp(&score(b))).unwrap_or(0);
    -(best as f32) * std::f32::consts::TAU / LOOKS as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(name: &str, at: [f32; 2], up: f32) -> PortalDef {
        PortalDef {
            name: name.into(),
            at,
            up,
            radius: 1.0,
            to: None,
            both: false,
            bearing: None,
            push: None,
        }
    }

    fn pair(both: bool) -> Portals {
        let defs = vec![
            PortalDef {
                to: Some("gallery".into()),
                bearing: Some(0.0),
                both,
                ..def("pit", [10.0, 10.0], 0.0)
            },
            PortalDef {
                bearing: Some(90.0),
                ..def("gallery", [40.0, 10.0], 8.0)
            },
            PortalDef {
                push: Some([0.0, 6.7, 0.0]),
                ..def("kick", [70.0, 10.0], 0.0)
            },
        ];
        let mut p = Portals::new(&defs, |w| panic!("{w}"));
        p.settle(0.0, |_, _, want| Some(want - 1.0), |_, _, far| far);
        p
    }

    /// A body that has just walked from `from` to `to`.
    fn walked(from: Vec3, to: Vec3) -> Player {
        let mut b = Player::new(to, 0.0);
        b.prev_pos = from;
        b
    }

    /// Stepping into a teleporter puts you at the far end facing its exit
    /// direction, running that way, and does not send you back.
    #[test]
    fn a_teleporter_sends_a_body_and_turns_it() {
        let mut ps = pair(false);
        let mut b = walked(Vec3::new(10.0, 0.0, 12.0), Vec3::new(10.0, 0.0, 10.0));
        b.body.vel = Vec3::new(0.0, 0.0, -6.0); // running north
        ps.carry(&mut b);
        assert!((b.pos.x - 40.0).abs() < 0.01 && (b.pos.z - 10.0).abs() < 0.01, "landed at {:?}", b.pos);
        assert!((b.pos.y - 8.05).abs() < 0.01, "landed at {}", b.pos.y);
        assert_eq!(b.prev_pos, b.pos, "it would be drawn streaking across the map");
        // Bearing 90 is east, and the six metres a second came with it.
        assert!((b.yaw + std::f32::consts::FRAC_PI_2).abs() < 0.01, "facing {}", b.yaw);
        assert!(b.body.vel.x > 5.9 && b.body.vel.z.abs() < 0.1, "speed went {:?}", b.body.vel);
        // Standing in the far end does not send it back.
        let at = b.pos;
        ps.carry(&mut b);
        assert_eq!(b.pos, at);
        // A body already standing in the near end is not sent either; one
        // that steps in is.
        let mut c = Player::new(Vec3::new(10.0, 0.0, 10.0), 0.0);
        ps.carry(&mut c);
        assert_eq!(c.pos.x, 10.0, "it fired on a body that never stepped in");
        let mut d = walked(Vec3::new(12.0, 0.0, 10.0), Vec3::new(10.5, 0.0, 10.0));
        ps.carry(&mut d);
        assert!((d.pos.x - 40.0).abs() < 0.01);
    }

    /// A one-way pair does not send back; a two-way one does, once the body
    /// has stepped off and on again, and it comes out facing the near end's
    /// own exit direction.
    #[test]
    fn a_two_way_pair_sends_back() {
        let far_in = || walked(Vec3::new(42.0, 8.0, 10.0), Vec3::new(40.0, 8.0, 10.0));
        let mut one = pair(false);
        let mut b = far_in();
        one.carry(&mut b);
        assert_eq!(b.pos.x, 40.0, "a one-way pair sent back");

        let mut two = pair(true);
        let mut b = walked(Vec3::new(10.0, 0.0, 12.0), Vec3::new(10.0, 0.0, 10.0));
        two.carry(&mut b);
        assert!((b.pos.x - 40.0).abs() < 0.01);
        two.carry(&mut b);
        assert!((b.pos.x - 40.0).abs() < 0.01, "it bounced straight back");
        let mut b = far_in();
        b.body.vel = Vec3::new(-4.0, 0.0, 0.0);
        two.carry(&mut b);
        assert!((b.pos.x - 10.0).abs() < 0.01 && b.pos.y < 0.1, "came back to {:?}", b.pos);
        // The pit's exit direction is north.
        assert!(b.yaw.abs() < 0.01 && b.body.vel.z < -3.9, "facing {} moving {:?}", b.yaw, b.body.vel);
    }

    /// A portal with no exit direction faces arrivals the open way.
    #[test]
    fn no_bearing_faces_the_open_way() {
        let defs = vec![
            PortalDef {
                to: Some("nook".into()),
                ..def("door", [0.0, 0.0], 0.0)
            },
            def("nook", [20.0, 0.0], 0.0),
        ];
        let mut ps = Portals::new(&defs, |w| panic!("{w}"));
        // Walled in on every side but the south.
        ps.settle(0.0, |_, _, want| Some(want - 1.0), |_, d: Vec3, far| if d.z > 0.9 { far } else { 0.5 });
        let mut b = walked(Vec3::new(2.0, 0.0, 0.0), Vec3::ZERO);
        ps.carry(&mut b);
        assert!((b.pos.x - 20.0).abs() < 0.01);
        assert!((ahead(b.yaw) - Vec3::Z).length() < 0.01, "facing {:?}", ahead(b.yaw));
    }

    /// A pad throws you: the vertical part replaces what you had, the level
    /// part adds to it.
    #[test]
    fn a_pad_throws_a_body() {
        let mut ps = pair(false);
        let mut b = walked(Vec3::new(72.0, 0.0, 10.0), Vec3::new(70.0, 0.0, 10.0));
        b.body.vel = Vec3::new(3.0, -9.0, 0.0);
        ps.carry(&mut b);
        assert_eq!(b.body.vel, Vec3::new(3.0, 6.7, 0.0));
        assert!(!b.body.on_ground);
        assert_eq!(b.pos, Vec3::new(70.0, 0.0, 10.0), "a pad moved the body");
        // Falling back onto it from above throws it again.
        let mut b = walked(Vec3::new(70.0, 3.0, 10.0), Vec3::new(70.0, 0.5, 10.0));
        ps.carry(&mut b);
        assert_eq!(b.body.vel.y, 6.7);
    }

    /// A body that is nowhere near is left alone, and so is one on another
    /// floor directly above.
    #[test]
    fn it_leaves_everything_else_alone() {
        let mut ps = pair(false);
        let mut away = walked(Vec3::new(22.0, 0.0, 10.0), Vec3::new(20.0, 0.0, 10.0));
        ps.carry(&mut away);
        assert_eq!(away.pos.x, 20.0);
        let mut over = walked(Vec3::new(12.0, 6.0, 10.0), Vec3::new(10.0, 6.0, 10.0));
        ps.carry(&mut over);
        assert_eq!(over.pos.y, 6.0);
    }
}
