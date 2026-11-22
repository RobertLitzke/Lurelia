//! The team brain as it stood on 2026-11-19, before the CTF tournament
//! (docs/BOTS.md): kept whole as the `base` coach, so the bench can
//! play what the tournament made against what it started from. Do not tune
//! this file; it is a fixed point.

#![allow(dead_code)]

use super::team::{Coach, Member, Seen, TeamCtx};
use super::{Goal, Order, Role, Sighting};
use glam::Vec3;

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

/// One side's planner.
#[derive(Default)]
pub struct TeamBrain {
    pub coach: Coach,
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
}

/// How many of `n` bots defend when nothing is happening.
fn defenders(n: usize) -> usize {
    match n {
        0 | 1 => 0,
        2..=4 => 1,
        _ => 2,
    }
}

impl TeamBrain {
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
                out.push((c, Order::new(Role::Carry, Goal::Point(cx.own.stand), 1.0)));
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
                Some((p, t)) if cx.now - t < CHASE_S => p,
                _ => cx.theirs.stand,
            };
            let k = (free.len() / 2).max(1);
            needs.push((Role::Retrieve, k, Goal::Point(goal), 0.8, Some(by)));
        }
        if let Seen::Dropped(at) = cx.theirs.seen {
            needs.push((Role::Attack, 1, Goal::Point(at), 0.9, None));
        }
        if let Some(c) = carrier {
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
        for k in 0..d {
            let post = cx.posts.get(k).copied().unwrap_or(cx.own.stand);
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
        // Everybody left goes for their flag, each by the next lane.
        let attack_to = match cx.theirs.seen {
            Seen::Dropped(at) => at,
            _ => cx.theirs.stand,
        };
        let mut used: Vec<Vec3> = Vec::new();
        let mut lane_of = Vec::new();
        for m in &free {
            let mut o = Order::new(Role::Attack, Goal::Point(attack_to), 0.7);
            if !cx.lanes.is_empty() && cx.theirs.seen == Seen::Home {
                let n = cx.lanes.len();
                // The lane it had, if nobody else has taken it this plan;
                // else the next one round that nobody has.
                let kept = self.lane_of.iter().find(|(w, _)| *w == m.who).map(|l| l.1).filter(|l| cx.lanes.contains(l) && !used.contains(l));
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
