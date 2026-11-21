//! Capture the flag: a flag on a stand at each side's base, which the other
//! side takes, drops when it dies and carries home to score.
//!
//! The rules own the flags the way they own the pads (`pickup.rs`), and this
//! file knows nothing about guns, brains or drawing. It is handed the bodies
//! each step (who, which side, where, whether alive) and answers what
//! happened, as a list of `Event`s that the HUD, the team brain and the
//! headless report all read. Nothing else decides when a flag moves.
//!
//! Three states and the moves between them:
//!
//! | From | What happens | To |
//! |---|---|---|
//! | Home | an enemy comes within reach | Carried |
//! | Carried | the carrier dies | Dropped where it fell |
//! | Dropped | one of the flag's side comes within reach | Home (a return) |
//! | Dropped | an enemy comes within reach | Carried |
//! | Dropped | `return_s` runs out, or it lies somewhere lost | Home |
//! | Carried | the carrier reaches their own stand, their own flag at home | Home, and a capture |

use crate::world::fields::CtfDef;
use glam::Vec3;

/// How near a body has to come to a flag or a stand to take it, metres
/// across the floor: the pads' reach, so a flag is walked over like a pad.
pub const REACH: f32 = 1.3;
/// How far apart in height a body and a flag may be and still touch.
const REACH_UP: f32 = 2.0;

/// Where a flag is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Home,
    /// On fighter `who`'s back.
    Carried(usize),
    /// Lying where its carrier fell, with seconds left before it goes home.
    Dropped {
        at: Vec3,
        left: f32,
    },
}

/// Something a flag did, for whoever is listening. `team` is always the
/// flag's own side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    /// Taken off its stand by `by`.
    Taken { team: u8, by: usize },
    /// Picked up off the floor by `by`, of the other side.
    PickedUp { team: u8, by: usize },
    /// Dropped by `by`, who died carrying it, at `at`.
    Dropped { team: u8, by: usize, at: Vec3 },
    /// Back on its stand: walked onto by `by`, or on its own when `by` is
    /// None.
    Returned { team: u8, by: Option<usize> },
    /// Carried home by `by`: a point to the other side.
    Captured { team: u8, by: usize },
}

impl Event {
    pub fn team(&self) -> u8 {
        match *self {
            Event::Taken { team, .. } | Event::PickedUp { team, .. } | Event::Dropped { team, .. } | Event::Returned { team, .. } | Event::Captured { team, .. } => team,
        }
    }
}

/// One body as the flags see it.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub who: usize,
    pub team: Option<u8>,
    pub at: Vec3,
    pub alive: bool,
}

pub struct Flag {
    pub team: u8,
    /// The stand, on its floor once `placed`.
    pub stand: Vec3,
    pub placed: bool,
    /// Metres `[x, z]` and the stated height, kept to look for the floor again.
    at: [f32; 2],
    up: Option<f32>,
    pub state: State,
}

#[derive(Default)]
pub struct Flags {
    pub flags: Vec<Flag>,
    /// Captures by each side.
    pub caps: [u32; 2],
    return_s: f32,
    need_home: bool,
}

fn near(a: Vec3, b: Vec3) -> bool {
    Vec3::new(a.x - b.x, 0.0, a.z - b.z).length() < REACH && (a.y - b.y).abs() < REACH_UP
}

impl Flags {
    /// The flags a world's `ctf` block names. `floor` finds the floor under
    /// a stand and says whether it found one, as `Shooter::pad_floor` does.
    pub fn new(def: &CtfDef, mut floor: impl FnMut(f32, f32, Option<f32>) -> (Vec3, bool), mut warn: impl FnMut(String)) -> Flags {
        let mut flags = Vec::new();
        for f in &def.flags {
            if f.team > 1 {
                warn(format!("a flag for team {}, and there are two teams, 0 and 1", f.team));
                continue;
            }
            if flags.iter().any(|g: &Flag| g.team == f.team) {
                warn(format!("two flags for team {}", f.team));
                continue;
            }
            let (stand, placed) = floor(f.at[0], f.at[1], f.up);
            flags.push(Flag {
                team: f.team,
                stand,
                placed,
                at: f.at,
                up: f.up,
                state: State::Home,
            });
        }
        if flags.len() != 2 {
            warn(format!("capture the flag wants a flag for each team and has {}", flags.len()));
        }
        Flags {
            flags,
            caps: [0, 0],
            return_s: def.return_s.max(1.0),
            need_home: def.need_home,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    /// Ask again for the floor under any stand whose column had not loaded.
    pub fn settle(&mut self, mut floor: impl FnMut(f32, f32, Option<f32>) -> (Vec3, bool)) {
        for f in self.flags.iter_mut().filter(|f| !f.placed) {
            let (at, found) = floor(f.at[0], f.at[1], f.up);
            f.stand = at;
            f.placed = found;
        }
    }

    pub fn unplaced(&self) -> usize {
        self.flags.iter().filter(|f| !f.placed).count()
    }

    pub fn of(&self, team: u8) -> Option<&Flag> {
        self.flags.iter().find(|f| f.team == team)
    }

    /// The side whose flag fighter `who` has on its back.
    pub fn carried_by(&self, who: usize) -> Option<u8> {
        self.flags.iter().find(|f| f.state == State::Carried(who)).map(|f| f.team)
    }

    /// Both flags home and nobody's score, for a new match.
    pub fn reset(&mut self) {
        self.caps = [0, 0];
        for f in &mut self.flags {
            f.state = State::Home;
        }
    }

    /// One step. `land` takes the point a carrier fell at to the floor under
    /// it; `lost` says whether a flag lying there cannot be got back
    /// (burning ground, the void), in which case it goes home at once.
    pub fn step(&mut self, dt: f32, bodies: &[Body], land: impl Fn(Vec3) -> Vec3, lost: impl Fn(Vec3) -> bool) -> Vec<Event> {
        let mut out = Vec::new();
        let alive = |who: usize| bodies.iter().find(|b| b.who == who).filter(|b| b.alive);
        // A carrier who has died drops what they carried where they fell.
        for f in &mut self.flags {
            if let State::Carried(who) = f.state {
                if alive(who).is_none() {
                    let fell = bodies.iter().find(|b| b.who == who).map_or(f.stand, |b| b.at);
                    let at = land(fell);
                    out.push(Event::Dropped { team: f.team, by: who, at });
                    if lost(at) {
                        f.state = State::Home;
                        out.push(Event::Returned { team: f.team, by: None });
                    } else {
                        f.state = State::Dropped { at, left: self.return_s };
                    }
                }
            }
            if let State::Dropped { at, left } = f.state {
                let left = left - dt;
                if left <= 0.0 {
                    f.state = State::Home;
                    out.push(Event::Returned { team: f.team, by: None });
                } else {
                    f.state = State::Dropped { at, left };
                }
            }
        }
        // Then whoever is standing on something.
        for b in bodies.iter().filter(|b| b.alive) {
            let Some(side) = b.team else { continue };
            for k in 0..self.flags.len() {
                let (team, state, stand) = (self.flags[k].team, self.flags[k].state, self.flags[k].stand);
                if team == side {
                    match state {
                        State::Dropped { at, .. } if near(b.at, at) => {
                            self.flags[k].state = State::Home;
                            out.push(Event::Returned { team, by: Some(b.who) });
                        }
                        // At their own stand with the other side's flag on
                        // their back.
                        State::Home | State::Carried(_) | State::Dropped { .. } if self.flags[k].placed && near(b.at, stand) => {
                            let home = state == State::Home;
                            if let Some(j) = self.flags.iter().position(|f| f.state == State::Carried(b.who)) {
                                if home || !self.need_home {
                                    let theirs = self.flags[j].team;
                                    self.flags[j].state = State::Home;
                                    self.caps[side.min(1) as usize] += 1;
                                    out.push(Event::Captured { team: theirs, by: b.who });
                                }
                            }
                        }
                        _ => {}
                    }
                } else if self.carried_by(b.who).is_none() {
                    match state {
                        State::Home if self.flags[k].placed && near(b.at, stand) => {
                            self.flags[k].state = State::Carried(b.who);
                            out.push(Event::Taken { team, by: b.who });
                        }
                        State::Dropped { at, .. } if near(b.at, at) => {
                            self.flags[k].state = State::Carried(b.who);
                            out.push(Event::PickedUp { team, by: b.who });
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::FlagDef;

    /// Cyan's stand at x = -20, magenta's at x = +20, on a flat floor.
    fn flags() -> Flags {
        let def = CtfDef {
            flags: vec![FlagDef { team: 0, at: [-20.0, 0.0], up: None }, FlagDef { team: 1, at: [20.0, 0.0], up: None }],
            captures: 3,
            time_s: 0.0,
            return_s: 10.0,
            need_home: true,
        };
        Flags::new(&def, |x, z, _| (Vec3::new(x, 0.0, z), true), |w| panic!("{w}"))
    }

    fn at(who: usize, team: u8, x: f32) -> Body {
        Body {
            who,
            team: Some(team),
            at: Vec3::new(x, 0.0, 0.0),
            alive: true,
        }
    }

    fn step(f: &mut Flags, bodies: &[Body]) -> Vec<Event> {
        f.step(0.1, bodies, |p| p, |_| false)
    }

    #[test]
    fn take_it_carry_it_home_and_score() {
        let mut f = flags();
        // Magenta's fighter 2 walks onto cyan's stand.
        assert_eq!(step(&mut f, &[at(2, 1, -20.0)]), vec![Event::Taken { team: 0, by: 2 }]);
        assert_eq!(f.carried_by(2), Some(0));
        // Walking about with it does nothing.
        assert!(step(&mut f, &[at(2, 1, 0.0)]).is_empty());
        // Home to its own stand, magenta's flag there: a point to magenta.
        assert_eq!(step(&mut f, &[at(2, 1, 20.0)]), vec![Event::Captured { team: 0, by: 2 }]);
        assert_eq!(f.caps, [0, 1]);
        assert_eq!(f.of(0).unwrap().state, State::Home, "the captured flag did not go home");
    }

    #[test]
    fn a_side_does_not_take_its_own_flag() {
        let mut f = flags();
        assert!(step(&mut f, &[at(1, 0, -20.0)]).is_empty());
        assert_eq!(f.of(0).unwrap().state, State::Home);
    }

    #[test]
    fn no_capture_while_your_own_flag_is_away() {
        let mut f = flags();
        step(&mut f, &[at(1, 0, 20.0), at(2, 1, -20.0)]);
        assert_eq!((f.carried_by(1), f.carried_by(2)), (Some(1), Some(0)), "both flags should be out");
        // Cyan's carrier reaches its own empty stand: no point.
        assert!(step(&mut f, &[at(1, 0, -20.0), at(2, 1, 0.0)]).is_empty(), "scored with its own flag away");
        assert_eq!(f.caps, [0, 0]);
    }

    #[test]
    fn a_dead_carrier_drops_it_and_a_teammate_returns_it() {
        let mut f = flags();
        step(&mut f, &[at(2, 1, -20.0)]);
        let mut dead = at(2, 1, -5.0);
        dead.alive = false;
        let e = step(&mut f, &[dead]);
        assert!(matches!(e[0], Event::Dropped { team: 0, by: 2, .. }), "{e:?}");
        assert!(matches!(f.of(0).unwrap().state, State::Dropped { .. }));
        // Cyan's fighter 1 walks onto it: home.
        assert_eq!(step(&mut f, &[at(1, 0, -5.0)]), vec![Event::Returned { team: 0, by: Some(1) }]);
        assert_eq!(f.of(0).unwrap().state, State::Home);
    }

    #[test]
    fn an_enemy_picks_a_dropped_flag_up_again() {
        let mut f = flags();
        step(&mut f, &[at(2, 1, -20.0)]);
        let mut dead = at(2, 1, -5.0);
        dead.alive = false;
        step(&mut f, &[dead]);
        assert_eq!(step(&mut f, &[at(3, 1, -5.0)]), vec![Event::PickedUp { team: 0, by: 3 }]);
        assert_eq!(f.carried_by(3), Some(0));
    }

    #[test]
    fn a_dropped_flag_goes_home_on_its_own_and_at_once_from_somewhere_lost() {
        let mut f = flags();
        step(&mut f, &[at(2, 1, -20.0)]);
        let mut dead = at(2, 1, -5.0);
        dead.alive = false;
        step(&mut f, &[dead]);
        let mut back = Vec::new();
        for _ in 0..101 {
            back.extend(step(&mut f, &[]));
        }
        assert_eq!(back, vec![Event::Returned { team: 0, by: None }], "ten seconds on the floor and it did not go home");
        // Dropped into something that burns: straight home.
        step(&mut f, &[at(2, 1, -20.0)]);
        let e = f.step(0.1, &[dead], |p| p, |_| true);
        assert_eq!(e.last(), Some(&Event::Returned { team: 0, by: None }));
        assert_eq!(f.of(0).unwrap().state, State::Home);
    }

    #[test]
    fn a_new_match_puts_both_home() {
        let mut f = flags();
        step(&mut f, &[at(2, 1, -20.0)]);
        step(&mut f, &[at(2, 1, 20.0)]);
        step(&mut f, &[at(2, 1, -20.0)]);
        f.reset();
        assert_eq!(f.caps, [0, 0]);
        assert!(f.flags.iter().all(|g| g.state == State::Home));
    }
}
