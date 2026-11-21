//! The team brain: what does my side need? (docs/BOTS.md, "The team".)
//!
//! A layer above the decision one. One per side, run by the rules before
//! the bots think, every second and at once whenever a flag moves. It reads
//! a `TeamCtx` -- both flags as everybody knows them, where its own side is,
//! and what its bots have seen between them -- and hands each of its bots an
//! `Order`. The bots do the rest (`duel::Duel::obey`).
//!
//! It knows no more than the side would. Where a flag is lying is public
//! (the HUD shows every player); where the enemy carrying our flag is, only
//! while one of our bots can see them, and the last place they were seen
//! after that. Pooled sightings are what a side on voice comms has, and no
//! more than any one of them saw.
//!
//! The plan is a list of needs in order of how badly they are wanted:
//!
//! | Need | When | How many |
//! |---|---|---|
//! | Carry | we have their flag | whoever has it |
//! | Return | our flag is lying somewhere | one, the nearest |
//! | Retrieve | they have our flag | half the side, at least one |
//! | Pick up | their flag is lying somewhere | one |
//! | Escort | we have their flag | one or two |
//! | Defend | always, fewer when busy; at midfield while nobody is near home | by the side's size |
//! | Attack | everybody left | the rest |
//!
//! and the bots are dealt into it nearest first, with a thumb on the scale
//! for the job a bot already has: a bot keeps its job for `HOLD_S` unless a
//! flag moves, because a side whose bots swap jobs every second looks like a
//! side where nobody has one.

use super::{Goal, Order, Role, Sighting};
use glam::Vec3;

/// Whether a tournament change is on (docs/BOTS.md "What the bench found"). Each one can be
/// switched off by naming it in `CTF_OFF` (comma-separated), so the bench
/// can play a change against its absence from one build: other work on the
/// code moves the numbers between builds, and only a paired run measures
/// the change and nothing else. Read once.
///
/// A change the tournament tried and turned down stays in the code but is
/// off unless named in `CTF_ON`: `REJECTED`.
pub fn trying(name: &str) -> bool {
    fn list(var: &str) -> Vec<String> {
        std::env::var(var).map(|v| v.split(',').map(|s| s.trim().to_string()).collect()).unwrap_or_default()
    }
    static OFF: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    static ON: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    if REJECTED.contains(&name) {
        return ON.get_or_init(|| list("CTF_ON")).iter().any(|o| o == name);
    }
    !OFF.get_or_init(|| list("CTF_OFF")).iter().any(|o| o == name)
}

/// Tournament changes that lost their paired round (docs/BOTS.md).
const REJECTED: [&str; 5] = ["overwatch", "react", "cutoff", "lead", "hide"];

/// Seconds between plans when nothing happens.
pub const PLAN_EVERY_S: f32 = 1.0;
/// The least a bot keeps a job before a plan with no flag news may change it.
const HOLD_S: f32 = 6.0;
/// What having the job already is worth in the deal, in metres of walk.
const STICK_M: f32 = 25.0;
/// How long the last sighting of our carrier is worth chasing rather than
/// cutting them off at their stand.
const CHASE_S: f32 = 5.0;
/// How far a defender ranges from its post.
const LEASH_M: f32 = 22.0;
/// An overwatch barely leaves its post, and picks its gun for this range.
const OVERWATCH_LEASH_M: f32 = 6.0;
const OVERWATCH_REACH_M: f32 = 40.0;

/// One of the side, as the team brain knows them: its own, so all of it.
#[derive(Clone, Copy, Debug)]
pub struct Member {
    pub who: usize,
    pub at: Vec3,
    pub alive: bool,
    /// 0..1 of its health.
    pub whole: f32,
    /// A bot takes orders; the player is planned around.
    pub bot: bool,
}

/// A flag as a side knows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seen {
    Home,
    /// On `by`'s back, at `at` if anybody on this side can see them.
    Carried {
        by: usize,
        at: Option<Vec3>,
    },
    Dropped(Vec3),
}

#[derive(Clone, Copy, Debug)]
pub struct FlagView {
    pub stand: Vec3,
    pub seen: Seen,
}

/// What a team brain may read.
pub struct TeamCtx<'a> {
    pub now: f32,
    pub members: &'a [Member],
    pub own: FlagView,
    pub theirs: FlagView,
    /// Places between the two stands that are different ways across: an
    /// attacker is sent by one, and two attackers by two (`lanes`).
    pub lanes: &'a [Vec3],
    /// Where its defenders stand, best first (`posts`).
    pub posts: &'a [Vec3],
    /// High ground on its own half over the middle, best first
    /// (`vantages`); empty on a flat map.
    pub vantages: &'a [Vec3],
    /// Captures each side needs no more of, and seconds left in the match:
    /// a side behind with a minute left sends everybody.
    pub behind: bool,
    pub left: Option<f32>,
    /// How many enemies there are, and how many of them the side has seen
    /// die in the last `RESET_S` (the kill feed is public): those are back
    /// at their own spawn, a walk away.
    pub foes: usize,
    pub reset: usize,
}

/// How a side is run: the shipping planner, and the two controls the CTF
/// bench measures it against (`tools::ctf`). Each control still carries a
/// flag home, because a bot with the flag on its back knows it has it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Coach {
    /// The plan above.
    #[default]
    Plan,
    /// Every bot gets a job at the start, every third one defending and the
    /// rest attacking, and keeps it whatever happens.
    Fixed,
    /// Everybody attacks, all the time.
    Rush,
    /// The plan as it was before the tournament (`team_v0`), for the bench
    /// to play the current one against.
    Base,
}

/// One side's planner.
#[derive(Default)]
pub struct TeamBrain {
    pub coach: Coach,
    /// The frozen planner, used when `coach` is `Base`.
    base: super::team_v0::TeamBrain,
    /// Each bot's job and when it got it.
    jobs: Vec<(usize, Role, f32)>,
    /// Seconds to the next plan.
    next: f32,
    /// Plans made, for the report.
    pub plans: u32,
    /// Where the enemy carrying our flag was last seen, and when.
    pub carrier_seen: Option<(Vec3, f32)>,
    /// Which lane the next new attacker is sent by, turned each time so the
    /// side does not always come down the same one, and the lane each
    /// attacker has, which it keeps while it attacks.
    lane_turn: usize,
    lane_of: Vec<(usize, Vec3)>,
    /// When an enemy was last seen near its stand, by anybody on the side.
    threat_at: f32,
    /// Stance changes under test (`brain::TWEAKS`: `stance`, `reset`,
    /// `group`), set for one side by the CTF bench.
    pub tweaks: u32,
}

/// How near its stand an enemy has to be seen, and how lately, for a
/// reactive defence to hold its posts.
const THREAT_M: f32 = 30.0;
const THREAT_S: f32 = 10.0;
/// An attacker's urgency when pressing: over `obey`'s 0.8 bar for breaking
/// off.
const ATTACK_PRESS: f32 = 0.85;
/// How far along the thief's way home, from where it was seen, retrievers
/// head to cut it off.
const CUTOFF_LEAD: f32 = 0.35;
/// How long a killed enemy counts as reset: respawned at its own end and
/// walking back.
pub const RESET_S: f32 = 10.0;
/// How far toward the enemy, as a share of the way between the stands, a
/// defender stands while nobody threatens the flag (`stance`).
const FORWARD: f32 = 0.4;
/// A grouped attacker this near the flag stops following and goes for it.
const GROUP_PUSH_M: f32 = 25.0;

/// How many of `n` bots defend when nothing is happening.
fn defenders(n: usize) -> usize {
    match n {
        0 | 1 => 0,
        2..=4 => 1,
        _ => 2,
    }
}

impl TeamBrain {
    fn on(&self, name: &str) -> bool {
        super::tweak_bit(name).is_some_and(|b| self.tweaks & b != 0)
    }

    /// Whether a plan is due: on the clock, or at once when a flag moved.
    pub fn due(&mut self, dt: f32, news: bool) -> bool {
        self.next -= dt;
        if news || self.next <= 0.0 {
            self.next = PLAN_EVERY_S;
            return true;
        }
        false
    }

    /// Hand out the jobs. `urgent` is a flag having moved since the last
    /// plan, which lets any bot change job at once. Returns an order per
    /// living bot of the side.
    pub fn plan(&mut self, cx: &TeamCtx, sightings: &[Sighting], urgent: bool) -> Vec<(usize, Order)> {
        if self.coach == Coach::Base {
            return self.base.plan(cx, sightings, urgent);
        }
        self.plans += 1;
        let bots: Vec<&Member> = cx.members.iter().filter(|m| m.bot && m.alive).collect();
        let mut out = Vec::with_capacity(bots.len());
        let mut free: Vec<&Member> = bots.clone();
        let take = |free: &mut Vec<&Member>, who: usize| free.retain(|m| m.who != who);

        // Our carrier, bot or player.
        let carrier = match cx.theirs.seen {
            Seen::Carried { by, .. } if cx.members.iter().any(|m| m.who == by) => Some(by),
            _ => None,
        };
        if let Some(c) = carrier {
            if free.iter().any(|m| m.who == c) {
                // In a standoff it cannot score: it waits off the stand, at
                // the side's first post, where it is not the first thing a
                // returning enemy walks into. Its own flag coming home is a
                // flag event, so the next plan sends it on at once.
                let goal = match (cx.own.seen, cx.posts.first()) {
                    (Seen::Carried { .. } | Seen::Dropped(_), Some(&post)) if trying("hide") => Goal::Hold { post, leash: 4.0, watch: cx.theirs.stand },
                    _ => Goal::Point(cx.own.stand),
                };
                out.push((c, Order::new(Role::Carry, goal, 1.0)));
                take(&mut free, c);
            }
        }

        // Where the enemy with our flag is, as far as the side knows.
        let thief = match cx.own.seen {
            Seen::Carried { by, at } => {
                let seen = at.or_else(|| {
                    sightings
                        .iter()
                        .filter(|s| s.who == by)
                        .max_by(|a, b| a.when.total_cmp(&b.when))
                        .filter(|s| cx.now - s.when < 0.35)
                        .map(|s| s.at)
                });
                if let Some(p) = seen {
                    self.carrier_seen = Some((p, cx.now));
                }
                Some(by)
            }
            _ => {
                self.carrier_seen = None;
                None
            }
        };

        if self.coach != Coach::Plan {
            let attack_to = match cx.theirs.seen {
                Seen::Dropped(at) => at,
                _ => cx.theirs.stand,
            };
            // Fixed: a job for life by the bot's place in its side, every
            // third one from the second defending, so a side of two or three
            // holds with one, as the plan would when nothing is happening.
            let side: Vec<usize> = cx.members.iter().filter(|m| m.bot).map(|m| m.who).collect();
            for m in &free {
                let place = side.iter().position(|&w| w == m.who).unwrap_or(0);
                let defend = self.coach == Coach::Fixed && place % 3 == 1;
                let o = if defend {
                    Order::new(
                        Role::Defend,
                        Goal::Hold {
                            post: cx.posts.first().copied().unwrap_or(cx.own.stand),
                            leash: LEASH_M,
                            watch: cx.theirs.stand,
                        },
                        0.3,
                    )
                } else {
                    Order::new(Role::Attack, Goal::Point(attack_to), 0.7)
                };
                out.push((m.who, o));
            }
            return out;
        }

        // The needs, most wanted first: (role, how many, goal, urgency).
        let mut needs: Vec<(Role, usize, Goal, f32, Option<usize>)> = Vec::new();
        if let Seen::Dropped(at) = cx.own.seen {
            needs.push((Role::Return, 1, Goal::Point(at), 0.9, None));
        }
        if let Some(by) = thief {
            // Chase the last sighting while it is fresh; after that, cut
            // them off where they are going, which is their own stand.
            let goal = match self.carrier_seen {
                // Cut-off: lead the thief along its way home rather than
                // run to where it was.
                Some((p, t)) if cx.now - t < CHASE_S && trying("cutoff") => p + (cx.theirs.stand - p) * CUTOFF_LEAD,
                Some((p, t)) if cx.now - t < CHASE_S => p,
                _ => cx.theirs.stand,
            };
            // A standoff, both flags out, is broken by whoever gets their
            // own flag back first, and the carrier cannot score until then:
            // everybody but the carrier goes. Otherwise half the side.
            let k = if carrier.is_some() && trying("standoff") { free.len() } else { (free.len() / 2).max(1) };
            needs.push((Role::Retrieve, k, Goal::Point(goal), 0.8, Some(by)));
        }
        if let Seen::Dropped(at) = cx.theirs.seen {
            needs.push((Role::Attack, 1, Goal::Point(at), 0.9, None));
        }
        // An escort only while the carrier can score: in a standoff the
        // carrier waits at home and its escort is needed after the thief.
        if let Some(c) = carrier.filter(|_| thief.is_none() || !trying("standoff")) {
            let k = if free.len() >= 4 { 2 } else { 1 };
            needs.push((Role::Escort, k.min(free.len()), Goal::Follow(c), 0.6, None));
        }
        // Defence: by the side's size, less a player holding the base,
        // none while the side is chasing its own flag (unless it is big),
        // and none when it is behind with a minute left.
        let player_home = cx.members.iter().any(|m| !m.bot && m.alive && m.at.distance(cx.own.stand) < 15.0);
        let mut d = defenders(bots.len()).saturating_sub(usize::from(player_home));
        if thief.is_some() && bots.len() < 4 {
            d = 0;
        }
        if cx.behind && cx.left.is_some_and(|t| t < 60.0) {
            d = 0;
        }
        // Reactive defence: hold posts only while the enemy has lately been
        // seen near home; otherwise a defender guards nothing and is worth
        // more going for their flag.
        if let Some(s) = sightings.iter().filter(|s| s.at.distance(cx.own.stand) < THREAT_M).max_by(|a, b| a.when.total_cmp(&b.when)) {
            self.threat_at = self.threat_at.max(s.when);
        }
        if trying("react") && cx.now - self.threat_at > THREAT_S {
            d = 0;
        }
        // Every enemy freshly killed and walking back from its spawn:
        // nobody is coming, so everybody goes (`reset`). Slightly ahead of
        // keeping the defender on the bench, within noise, and kept for the
        // livelier game; on unless `CTF_OFF=reset`.
        let home_safe = cx.own.seen == Seen::Home && thief.is_none();
        if (trying("reset") || self.on("reset")) && home_safe && cx.foes > 0 && cx.reset >= cx.foes {
            d = 0;
        }
        // Nobody seen near the flag lately: the defence stands forward, at
        // midfield, where it meets an attack early and can still drop back
        // (`stance`). An enemy seen near home brings it back to the posts.
        // Level with the defence at home on the bench, and kept because it
        // makes for a livelier game; on unless `CTF_OFF=stance`.
        let forward = (trying("stance") || self.on("stance")) && home_safe && (cx.now - self.threat_at > THREAT_S || self.threat_at == 0.0);
        for k in 0..d {
            let post = if forward {
                cx.own.stand + (cx.theirs.stand - cx.own.stand) * FORWARD
            } else {
                cx.posts.get(k).copied().unwrap_or(cx.own.stand)
            };
            needs.push((
                Role::Defend,
                1,
                Goal::Hold {
                    post,
                    leash: LEASH_M,
                    watch: cx.theirs.stand,
                },
                0.3,
                None,
            ));
        }

        // Deal the needs, nearest first, then everybody left attacks.
        let now = cx.now;
        let jobs = &self.jobs;
        let had = |who: usize| jobs.iter().find(|j| j.0 == who).copied();
        for (role, n, goal, urgency, mark) in needs {
            for _ in 0..n {
                if free.is_empty() {
                    break;
                }
                let at = |g: Goal| match g {
                    Goal::Point(p) => p,
                    Goal::Hold { post, .. } => post,
                    Goal::Follow(w) => cx.members.iter().find(|m| m.who == w).map_or(cx.own.stand, |m| m.at),
                };
                let target = at(goal);
                // A bot that has held another job a short while keeps it
                // unless a flag moved: it is passed over here.
                let settled = |m: &Member| !urgent && had(m.who).is_some_and(|(_, r, since)| r != role && now - since < HOLD_S);
                let pick = free
                    .iter()
                    .filter(|m| !settled(m))
                    .min_by(|a, b| {
                        let cost = |m: &Member| m.at.distance(target) - if had(m.who).is_some_and(|j| j.1 == role) { STICK_M } else { 0.0 } - m.whole * 5.0;
                        cost(a).total_cmp(&cost(b))
                    })
                    .map(|m| m.who);
                let Some(who) = pick else { break };
                let mut o = Order::new(role, goal, urgency);
                o.mark = mark;
                out.push((who, o));
                take(&mut free, who);
            }
        }
        // One of a side of three or more holds high ground over the middle
        // while nothing is happening, if the map has any.
        if let (Some(&v), None, None) = (cx.vantages.first(), thief, carrier) {
            if bots.len() >= 3 && free.len() >= 2 && trying("overwatch") {
                let pick = free.iter().min_by(|a, b| a.at.distance(v).total_cmp(&b.at.distance(v))).map(|m| m.who);
                if let Some(who) = pick {
                    let middle = (cx.own.stand + cx.theirs.stand) * 0.5;
                    let mut o = Order::new(Role::Overwatch, Goal::Hold { post: v, leash: OVERWATCH_LEASH_M, watch: middle }, 0.4);
                    o.reach = Some(OVERWATCH_REACH_M);
                    out.push((who, o));
                    take(&mut free, who);
                }
            }
        }
        // Everybody left goes for their flag, each by the next lane.
        let attack_to = match cx.theirs.seen {
            Seen::Dropped(at) => at,
            _ => cx.theirs.stand,
        };
        let mut used: Vec<Vec3> = Vec::new();
        let mut lane_of = Vec::new();
        // Attackers go together (`group`): the one furthest forward leads
        // by its lane, the rest follow it, and each goes for the flag on
        // its own once near it. Nobody waits for anybody.
        let flat = |a: Vec3, b: Vec3| Vec3::new(a.x - b.x, 0.0, a.z - b.z).length();
        let leader = (self.on("group") && free.len() >= 2 && cx.theirs.seen == Seen::Home)
            .then(|| free.iter().filter(|m| m.alive).min_by(|a, b| flat(a.at, attack_to).total_cmp(&flat(b.at, attack_to))).map(|m| m.who))
            .flatten();
        if let Some(l) = leader {
            free.retain(|m| {
                if m.who == l || flat(m.at, attack_to) < GROUP_PUSH_M {
                    return true;
                }
                out.push((m.who, Order::new(Role::Attack, Goal::Follow(l), if trying("press") { ATTACK_PRESS } else { 0.7 })));
                false
            });
        }
        for m in &free {
            // Pressing: urgent enough that an attacker does not break off or
            // chase across the map, and fights only what is in its way.
            let mut o = Order::new(Role::Attack, Goal::Point(attack_to), if trying("press") { ATTACK_PRESS } else { 0.7 });
            if !cx.lanes.is_empty() && cx.theirs.seen == Seen::Home {
                let n = cx.lanes.len();
                // The lane it had, if nobody else has taken it this plan;
                // else the next one round that nobody has.
                // `lane`: every attacker by the same lane, the one the first
                // of them was given, each making its own way along it.
                let shared = if self.on("lane") { used.first().copied() } else { None };
                let kept = shared.or_else(|| self.lane_of.iter().find(|(w, _)| *w == m.who).map(|l| l.1).filter(|l| cx.lanes.contains(l) && !used.contains(l)));
                let lane = kept.or_else(|| {
                    let pick = (0..n).map(|k| cx.lanes[(self.lane_turn + k) % n]).find(|l| !used.contains(l));
                    self.lane_turn += 1;
                    pick
                });
                let lane = lane.unwrap_or(cx.lanes[self.lane_turn % n]);
                used.push(lane);
                lane_of.push((m.who, lane));
                o.via = Some(lane);
            }
            out.push((m.who, o));
        }
        self.lane_of = lane_of;
        // Remember the jobs, keeping the time a job began for those whose
        // job is unchanged.
        let mut jobs = Vec::with_capacity(out.len());
        for (who, o) in &out {
            let since = self.jobs.iter().find(|j| j.0 == *who && j.1 == o.role).map_or(now, |j| j.2);
            jobs.push((*who, o.role, since));
        }
        self.jobs = jobs;
        out
    }
}

/// Places between two stands that are different ways across: nodes about
/// as far from one stand as from the other, no two of them within 8 m.
pub fn lanes(nodes: &[Vec3], a: Vec3, b: Vec3) -> Vec<Vec3> {
    let span = a.distance(b).max(1.0);
    let mut mid: Vec<Vec3> = nodes.iter().copied().filter(|n| (n.distance(a) - n.distance(b)).abs() < 0.2 * span).collect();
    // Furthest from the straight line first, so the lanes that are most
    // unlike each other are kept when there are too many.
    let line = (b - a).normalize_or(Vec3::X);
    let off = |p: Vec3| {
        let v = p - a;
        (v - line * v.dot(line)).length()
    };
    mid.sort_by(|x, y| off(*y).total_cmp(&off(*x)));
    let mut out: Vec<Vec3> = Vec::new();
    for p in mid {
        if out.iter().all(|q| q.distance(p) > 8.0) {
            out.push(p);
        }
        if out.len() == 4 {
            break;
        }
    }
    out
}

/// High ground for an overwatch: nodes on a side's own half at least 1.5 m
/// above its stand, highest first and then nearest the middle. Empty on a
/// flat map, where there is nothing to hold.
pub fn vantages(nodes: &[Vec3], stand: Vec3, enemy: Vec3) -> Vec<Vec3> {
    let middle = (stand + enemy) * 0.5;
    let mut out: Vec<Vec3> = nodes.iter().copied().filter(|n| n.y - stand.y >= 1.5 && n.distance(stand) < n.distance(enemy)).collect();
    out.sort_by(|a, b| {
        let score = |p: &Vec3| (p.y - stand.y) * 3.0 - Vec3::new(p.x - middle.x, 0.0, p.z - middle.z).length() * 0.2;
        score(b).total_cmp(&score(a))
    });
    out
}

/// Where a side's defenders stand: nodes between 5 and 16 m from its stand,
/// those nearest the way the enemy comes first, no two within 5 m.
pub fn posts(nodes: &[Vec3], stand: Vec3, enemy: Vec3) -> Vec<Vec3> {
    let mut near: Vec<Vec3> = nodes
        .iter()
        .copied()
        .filter(|n| {
            let d = Vec3::new(n.x - stand.x, 0.0, n.z - stand.z).length();
            (5.0..16.0).contains(&d) && (n.y - stand.y).abs() < 3.0
        })
        .collect();
    near.sort_by(|x, y| x.distance(enemy).total_cmp(&y.distance(enemy)));
    let mut out: Vec<Vec3> = Vec::new();
    for p in near {
        if out.iter().all(|q| q.distance(p) > 5.0) {
            out.push(p);
        }
    }
    if out.is_empty() {
        out.push(stand);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(who: usize, x: f32) -> Member {
        Member {
            who,
            at: Vec3::new(x, 0.0, 0.0),
            alive: true,
            whole: 1.0,
            bot: who != 0,
        }
    }

    fn cx<'a>(members: &'a [Member], own: Seen, theirs: Seen, lanes: &'a [Vec3], posts: &'a [Vec3]) -> TeamCtx<'a> {
        TeamCtx {
            now: 10.0,
            members,
            own: FlagView {
                stand: Vec3::new(-40.0, 0.0, 0.0),
                seen: own,
            },
            theirs: FlagView {
                stand: Vec3::new(40.0, 0.0, 0.0),
                seen: theirs,
            },
            lanes,
            posts,
            vantages: &[],
            behind: false,
            left: None,
            foes: 3,
            reset: 0,
        }
    }

    fn roles(out: &[(usize, Order)]) -> Vec<Role> {
        let mut r: Vec<Role> = out.iter().map(|(_, o)| o.role).collect();
        r.sort_by_key(|r| *r as u8);
        r
    }

    #[test]
    fn with_nothing_happening_a_side_of_three_attacks_and_holds() {
        let ms = [member(1, -30.0), member(2, -20.0), member(3, -10.0)];
        let posts = [Vec3::new(-32.0, 0.0, 0.0)];
        let lanes = [Vec3::new(0.0, 0.0, -10.0), Vec3::new(0.0, 0.0, 10.0)];
        let mut tb = TeamBrain::default();
        let out = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &lanes, &posts), &[], true);
        assert_eq!(roles(&out), vec![Role::Attack, Role::Attack, Role::Defend]);
        // With nobody seen near home the defender stands forward, so it is
        // the one nearest midfield; the attackers take two lanes.
        assert!(out.iter().any(|(w, o)| *w == 3 && o.role == Role::Defend), "{out:?}");
        let vias: Vec<_> = out.iter().filter_map(|(_, o)| o.via).collect();
        assert_eq!(vias.len(), 2);
        assert_ne!(vias[0], vias[1], "two attackers were sent down one lane");
    }

    #[test]
    fn a_carrier_carries_and_somebody_goes_with_them() {
        let ms = [member(1, 30.0), member(2, 20.0), member(3, -30.0)];
        let mut tb = TeamBrain::default();
        let out = tb.plan(&cx(&ms, Seen::Home, Seen::Carried { by: 1, at: None }, &[], &[]), &[], true);
        let job = |w: usize| out.iter().find(|(x, _)| *x == w).map(|(_, o)| *o).unwrap();
        assert_eq!(job(1).role, Role::Carry);
        assert_eq!(job(1).goal, Goal::Point(Vec3::new(-40.0, 0.0, 0.0)), "the carrier is not going home");
        assert_eq!(job(2).role, Role::Escort, "the bot beside the carrier did not escort: {out:?}");
        assert_eq!(job(2).goal, Goal::Follow(1));
    }

    #[test]
    fn our_flag_taken_sends_half_the_side_after_it_and_the_thief_is_marked() {
        let ms = [member(1, -30.0), member(2, -20.0), member(3, 0.0), member(4, 10.0)];
        let mut tb = TeamBrain::default();
        let seen = [Sighting {
            who: 7,
            at: Vec3::new(-10.0, 0.0, 5.0),
            when: 10.0,
        }];
        let out = tb.plan(&cx(&ms, Seen::Carried { by: 7, at: None }, Seen::Home, &[], &[]), &seen, true);
        let after: Vec<&Order> = out.iter().map(|(_, o)| o).filter(|o| o.role == Role::Retrieve).collect();
        assert_eq!(after.len(), 2, "{out:?}");
        assert!(after.iter().all(|o| o.mark == Some(7) && o.goal == Goal::Point(Vec3::new(-10.0, 0.0, 5.0))));
        // Unseen for a while: cut them off at their own stand.
        let later = TeamCtx {
            now: 20.0,
            ..cx(&ms, Seen::Carried { by: 7, at: None }, Seen::Home, &[], &[])
        };
        let out = tb.plan(&later, &[], true);
        assert!(out.iter().any(|(_, o)| o.role == Role::Retrieve && o.goal == Goal::Point(Vec3::new(40.0, 0.0, 0.0))), "{out:?}");
    }

    #[test]
    fn a_dropped_flag_is_walked_onto_by_the_nearest() {
        let ms = [member(1, -35.0), member(2, 0.0), member(3, 30.0)];
        let mut tb = TeamBrain::default();
        let out = tb.plan(&cx(&ms, Seen::Dropped(Vec3::new(-5.0, 0.0, 0.0)), Seen::Dropped(Vec3::new(25.0, 0.0, 0.0)), &[], &[]), &[], true);
        let job = |w: usize| out.iter().find(|(x, _)| *x == w).map(|(_, o)| *o).unwrap();
        assert_eq!(job(2).role, Role::Return);
        assert_eq!(job(3).role, Role::Attack);
        assert_eq!(job(3).goal, Goal::Point(Vec3::new(25.0, 0.0, 0.0)));
    }

    /// A bot keeps a job it has just been given when nothing has happened,
    /// even if another one now looks a little better.
    #[test]
    fn a_job_is_kept_a_while_without_news() {
        let ms = [member(1, -30.0), member(2, -20.0), member(3, -10.0)];
        let mut tb = TeamBrain::default();
        let first = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &[], &[]), &[], true);
        let defender = first.iter().find(|(_, o)| o.role == Role::Defend).unwrap().0;
        // The defender has wandered furthest out; one second later and no news.
        let moved: Vec<Member> = ms.iter().map(|m| if m.who == defender { member(m.who, 5.0) } else { *m }).collect();
        let mut c = cx(&moved, Seen::Home, Seen::Home, &[], &[]);
        c.now = 11.0;
        let again = tb.plan(&c, &[], false);
        assert_eq!(
            again.iter().find(|(_, o)| o.role == Role::Defend).unwrap().0,
            defender,
            "the defence changed hands with nothing happening"
        );
    }

    #[test]
    fn lanes_are_different_ways_across() {
        let a = Vec3::new(-40.0, 0.0, 0.0);
        let b = Vec3::new(40.0, 0.0, 0.0);
        let nodes = [
            Vec3::new(0.0, 0.0, -15.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 15.0),
            Vec3::new(2.0, 0.0, 15.0),
            Vec3::new(-30.0, 0.0, 0.0),
        ];
        let l = lanes(&nodes, a, b);
        assert_eq!(l.len(), 3, "{l:?}");
        let p = posts(&nodes, a, b);
        assert_eq!(p, vec![Vec3::new(-30.0, 0.0, 0.0)]);
    }

    /// A standoff, both flags out: everybody but the carrier goes after the
    /// thief, and nobody escorts a carrier who cannot score.
    #[test]
    fn in_a_standoff_everybody_but_the_carrier_retrieves() {
        let ms = [member(1, -38.0), member(2, -10.0), member(3, 0.0), member(4, 10.0)];
        let mut tb = TeamBrain::default();
        let c = cx(&ms, Seen::Carried { by: 7, at: Some(Vec3::new(20.0, 0.0, 0.0)) }, Seen::Carried { by: 1, at: None }, &[], &[]);
        let out = tb.plan(&c, &[], true);
        assert_eq!(roles(&out), vec![Role::Carry, Role::Retrieve, Role::Retrieve, Role::Retrieve], "{out:?}");
    }

    /// Attackers press: urgent enough that they do not break off or chase.
    #[test]
    fn attackers_press() {
        let ms = [member(1, -30.0), member(2, -20.0), member(3, -10.0)];
        let mut tb = TeamBrain::default();
        let out = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &[], &[]), &[], true);
        assert!(out.iter().filter(|(_, o)| o.role == Role::Attack).all(|(_, o)| o.urgency >= 0.8), "{out:?}");
    }

    fn with(names: &[&str]) -> TeamBrain {
        let (tweaks, bad) = super::super::tweak_mask(&names.join(","));
        assert!(bad.is_empty());
        TeamBrain { tweaks, ..TeamBrain::default() }
    }

    #[test]
    fn with_nobody_near_home_the_defence_stands_forward_and_drops_back_when_somebody_is() {
        let ms = [member(1, -35.0), member(2, -30.0), member(3, -25.0)];
        let posts = [Vec3::new(-30.0, 0.0, 0.0)];
        let post = |out: &[(usize, Order)]| match out.iter().find(|(_, o)| o.role == Role::Defend).map(|(_, o)| o.goal) {
            Some(Goal::Hold { post, .. }) => post,
            g => panic!("no defender: {g:?}"),
        };
        let mut tb = with(&["stance"]);
        let calm = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &[], &posts), &[], true);
        assert!(post(&calm).x > -15.0, "the defender stayed home with nobody about: {calm:?}");
        let near = [Sighting { who: 9, at: Vec3::new(-20.0, 0.0, 0.0), when: 10.0 }];
        let threat = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &[], &posts), &near, true);
        assert_eq!(post(&threat), posts[0], "the defender stayed forward with an enemy near the flag");
    }

    #[test]
    fn with_every_enemy_just_killed_everybody_attacks() {
        let ms = [member(1, -35.0), member(2, -30.0), member(3, -25.0)];
        let mut tb = with(&["reset"]);
        let wiped = TeamCtx { reset: 3, ..cx(&ms, Seen::Home, Seen::Home, &[], &[]) };
        assert!(tb.plan(&wiped, &[], true).iter().all(|(_, o)| o.role == Role::Attack));
        let two = TeamCtx { reset: 2, ..cx(&ms, Seen::Home, Seen::Home, &[], &[]) };
        assert!(tb.plan(&two, &[], true).iter().any(|(_, o)| o.role == Role::Defend));
    }

    #[test]
    fn grouped_attackers_follow_the_one_furthest_forward_until_near_the_flag() {
        let ms = [member(1, -35.0), member(2, -10.0), member(3, 0.0), member(4, 20.0)];
        let lanes = [Vec3::new(0.0, 0.0, 20.0), Vec3::new(0.0, 0.0, -20.0)];
        let mut tb = with(&["group"]);
        let out = tb.plan(&cx(&ms, Seen::Home, Seen::Home, &lanes, &[]), &[], true);
        let job = |w: usize| out.iter().find(|(x, _)| *x == w).map(|(_, o)| *o).unwrap();
        // 4 is 20 m from their stand, inside the push: it goes for the flag,
        // and leads. The others behind it follow it.
        assert_eq!(job(4).goal, Goal::Point(Vec3::new(40.0, 0.0, 0.0)));
        let followers: Vec<usize> = out.iter().filter(|(_, o)| o.role == Role::Attack && o.goal == Goal::Follow(4)).map(|(w, _)| *w).collect();
        assert_eq!(followers.len(), 2, "{out:?}");
    }
}
