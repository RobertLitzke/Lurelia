//! Places that do something to whoever steps into them: a teleporter that
//! puts you somewhere else facing a new way, and a pad that throws you.
//!
//! Both are the same shape of thing -- a standing cylinder that acts on a
//! body whose feet are inside it -- so they are one list in `world.json`
//! rather than two:
//!
//! ```json
//! "portals": [
//!   {"name": "pit", "at": [110.0, 92.0], "up": 0.0, "to": "gallery"},
//!   {"name": "gallery", "at": [148.0, 60.0], "up": 11.0, "bearing": 180},
//!   {"name": "kick", "at": [96.0, 40.0], "up": 0.0, "push": [0, 6.7, 0]}
//! ]
//! ```
//!
//! `at` and `up` place the foot of the cylinder, `up` measured from the floor
//! the world's own spawn point stands on, as nav nodes and pickup pads are.
//! `to` names the portal a body is sent to; `bearing` is the way it faces on
//! arrival, and its speed turns with it, so a teleporter you run into at full
//! tilt puts you out running. `push` is metres a second: the vertical part
//! replaces what the body had (that is what makes a jump pad a jump pad) and
//! the level part is added to it.
//!
//! A portal that is only a destination needs no `to`. One that is both ends
//! of a pair gets a cooldown when it is used, so arriving does not
//! immediately send you back.

use crate::player::Player;
use glam::Vec3;
use serde::Deserialize;

/// How long a portal ignores bodies after one arrives in it, seconds.
const COOL_S: f32 = 0.6;
/// How far above the foot of a portal a body still counts as in it.
const REACH_UP: f32 = 2.0;
/// And below, so a body dropping into one is caught.
const REACH_DOWN: f32 = 0.5;

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
    /// The way a body faces on arrival, degrees clockwise from north. Its
    /// speed turns with it.
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
    cool: f32,
    /// How many bodies it has sent or thrown, for the headless report.
    pub used: u32,
}

/// What a portal does to the body that stepped into it.
pub enum Step {
    /// Go there, facing that way, with your speed turned by `turn` radians.
    Go { to: Vec3, yaw: Option<f32>, turn: f32 },
    /// Be thrown: the level part is added, the vertical part replaces.
    Push(Vec3),
}

#[derive(Default)]
pub struct Portals {
    pub list: Vec<Portal>,
    defs: Vec<PortalDef>,
    placed: bool,
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
                cool: 0.0,
                used: 0,
            });
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
    /// the same question the shooter asks of its nav nodes.
    pub fn settle(&mut self, base: f32, floor: impl Fn(f32, f32, f32) -> Option<f32>) {
        if self.placed || self.list.is_empty() {
            return;
        }
        self.placed = true;
        for (p, d) in self.list.iter_mut().zip(&self.defs) {
            let want = base + d.up;
            let y = floor(d.at[0], d.at[1], want + 1.0).unwrap_or(want);
            p.at = Vec3::new(d.at[0], y, d.at[1]);
        }
    }

    pub fn step(&mut self, dt: f32) {
        for p in &mut self.list {
            p.cool = (p.cool - dt).max(0.0);
        }
    }

    /// What should happen to a body with its feet at `p`, if anything.
    /// Using a portal cools its far end, so a pair does not bounce a body
    /// back and forth.
    pub fn enter(&mut self, p: Vec3) -> Option<Step> {
        if !self.placed {
            return None;
        }
        let i = self.list.iter().position(|q| {
            q.cool <= 0.0 && (q.to.is_some() || q.push.is_some()) && p.y < q.at.y + REACH_UP && p.y > q.at.y - REACH_DOWN && (p.x - q.at.x).powi(2) + (p.z - q.at.z).powi(2) < q.radius * q.radius
        })?;
        self.list[i].used += 1;
        if let Some(j) = self.list[i].to {
            let turn = match (self.list[i].yaw, self.list[j].yaw) {
                (Some(from), Some(to)) => to - from,
                _ => 0.0,
            };
            self.list[j].cool = COOL_S;
            self.list[i].cool = COOL_S;
            return Some(Step::Go {
                to: self.list[j].at,
                yaw: self.list[j].yaw,
                turn,
            });
        }
        self.list[i].push.map(Step::Push)
    }

    /// Act on one body. Separate from `enter` so the rules for moving a body
    /// are written once, for the player and for every bot alike.
    pub fn carry(&mut self, b: &mut Player) {
        let Some(step) = self.enter(b.pos) else { return };
        match step {
            Step::Go { to, yaw, turn } => {
                b.pos = to + Vec3::Y * 0.05;
                // Drawn from where it was, so it has to come along or the
                // body is drawn streaking across the map.
                b.prev_pos = b.pos;
                // Off the floor it was standing on, until the next step finds
                // the one it has landed on.
                b.body.on_ground = false;
                if let Some(y) = yaw {
                    b.yaw = y;
                }
                let (s, c) = turn.sin_cos();
                let v = b.body.vel;
                b.body.vel = Vec3::new(v.x * c + v.z * s, v.y, -v.x * s + v.z * c);
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
                    "used": p.used,
                })
            })
            .collect();
        serde_json::Value::Array(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair() -> Portals {
        let defs = vec![
            PortalDef {
                name: "pit".into(),
                at: [10.0, 10.0],
                up: 0.0,
                radius: 1.0,
                to: Some("gallery".into()),
                bearing: Some(0.0),
                push: None,
            },
            PortalDef {
                name: "gallery".into(),
                at: [40.0, 10.0],
                up: 8.0,
                radius: 1.0,
                to: None,
                bearing: Some(90.0),
                push: None,
            },
            PortalDef {
                name: "kick".into(),
                at: [70.0, 10.0],
                up: 0.0,
                radius: 1.0,
                to: None,
                bearing: None,
                push: Some([0.0, 6.7, 0.0]),
            },
        ];
        let mut p = Portals::new(&defs, |w| panic!("{w}"));
        p.settle(0.0, |_, _, want| Some(want - 1.0));
        p
    }

    /// Stepping into a teleporter puts you at the far end facing its way,
    /// with your speed turned to match, and does not send you back.
    #[test]
    fn a_teleporter_sends_a_body_and_turns_it() {
        let mut ps = pair();
        let mut b = Player::new(Vec3::new(10.0, 0.0, 10.0), 0.0);
        b.body.vel = Vec3::new(0.0, 0.0, -6.0); // running north
        ps.carry(&mut b);
        assert!((b.pos.x - 40.0).abs() < 0.01 && (b.pos.z - 10.0).abs() < 0.01, "landed at {:?}", b.pos);
        assert!((b.pos.y - 8.05).abs() < 0.01, "landed at {}", b.pos.y);
        assert_eq!(b.prev_pos, b.pos, "it would be drawn streaking across the map");
        // Bearing 90 is east, and the six metres a second came with it.
        assert!(b.body.vel.x > 5.9 && b.body.vel.z.abs() < 0.1, "speed went {:?}", b.body.vel);
        // Standing in the far end does not send it back.
        let at = b.pos;
        ps.carry(&mut b);
        assert_eq!(b.pos, at);
        // Nor does the near end fire again straight away.
        let mut c = Player::new(Vec3::new(10.0, 0.0, 10.0), 0.0);
        ps.carry(&mut c);
        assert_eq!(c.pos.x, 10.0, "it fired again inside its cooldown");
        // Once the cooldown is out, it works again.
        for _ in 0..60 {
            ps.step(1.0 / 30.0);
        }
        ps.carry(&mut c);
        assert!((c.pos.x - 40.0).abs() < 0.01);
    }

    /// A pad throws you: the vertical part replaces what you had, the level
    /// part adds to it.
    #[test]
    fn a_pad_throws_a_body() {
        let mut ps = pair();
        let mut b = Player::new(Vec3::new(70.0, 0.0, 10.0), 0.0);
        b.body.vel = Vec3::new(3.0, -9.0, 0.0);
        ps.carry(&mut b);
        assert_eq!(b.body.vel, Vec3::new(3.0, 6.7, 0.0));
        assert!(!b.body.on_ground);
        assert_eq!(b.pos, Vec3::new(70.0, 0.0, 10.0), "a pad moved the body");
    }

    /// A body that is nowhere near is left alone, and so is one on another
    /// floor directly above.
    #[test]
    fn it_leaves_everything_else_alone() {
        let mut ps = pair();
        let mut away = Player::new(Vec3::new(20.0, 0.0, 10.0), 0.0);
        ps.carry(&mut away);
        assert_eq!(away.pos.x, 20.0);
        let mut over = Player::new(Vec3::new(10.0, 6.0, 10.0), 0.0);
        ps.carry(&mut over);
        assert_eq!(over.pos.y, 6.0);
    }
}
