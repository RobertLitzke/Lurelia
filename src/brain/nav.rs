//! Where a bot can go: a graph of places and what it takes to get between
//! them.
//!
//! The graph is baked, not searched at run time. A world writes its nodes
//! and links into the `nav` block of its `shooter` rules — Slagdeck's are
//! written by `worlds/slagdeck/make_arena.py`, the same script that builds
//! the deck, so the graph cannot drift away from the geometry it describes.
//!
//! The field that matters is `reach`: a link says what it takes to traverse
//! it, the search filters links against what this body can do, and so a bot
//! that cannot jump never plans a jump. That is also a difficulty lever with
//! no new code in it — a novice is simply not allowed the harder links.
//!
//! Nav points closer than about 1.5 m make bots stick on them, and nodes
//! made at run time are not in the network at all. So: keep nodes sparse,
//! and bake them.
//!
//! The A\* in `src/world/route/cost.rs` is a different question — it searches
//! a height field for a *builder*, asking what a road may be laid along.
//! This asks what a body may walk. One search would serve neither well.

use crate::world::fields::NavDef;
use glam::Vec3;
use std::collections::BinaryHeap;

/// What it takes to get along a link.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    /// Flat enough, or a ramp: anything can walk it.
    Walk,
    /// A step up that needs a jump.
    Jump,
    /// A fall: it goes one way only, and only a bot willing to take the
    /// drop uses it.
    Drop,
    /// Stand on a moving platform and be carried (src/platforms.rs). The
    /// only link that is a *wait* rather than a move: everything else in
    /// steering turns a want into keys, and this one turns it into standing
    /// still on the right square until the floor arrives somewhere else.
    Lift,
    /// Step into a portal and come out of its pair (src/portals.rs). A move
    /// like any other as far as the feet are concerned -- walk in and the
    /// world does the rest -- but one way, since the far end sends you back
    /// only if it is itself a portal.
    Portal,
}

impl Reach {
    pub fn parse(s: &str) -> Option<Reach> {
        match s {
            "walk" | "" => Some(Reach::Walk),
            "jump" => Some(Reach::Jump),
            "drop" => Some(Reach::Drop),
            "lift" => Some(Reach::Lift),
            "portal" => Some(Reach::Portal),
            _ => None,
        }
    }
}

/// What a body is willing and able to do.
#[derive(Clone, Copy, Debug)]
pub struct Able {
    pub jump: bool,
    /// Whether it will stand and wait for a lift, and whether it will step
    /// into a portal. Both true for anything that plays a map with them;
    /// kept as switches because a bot that has just been stranded on a dead
    /// plate should be able to stop trying.
    pub lift: bool,
    pub portal: bool,
    pub drop: bool,
}

impl Default for Able {
    fn default() -> Self {
        Able {
            jump: true,
            drop: true,
            lift: true,
            portal: true,
        }
    }
}

impl Able {
    pub fn can(&self, r: Reach) -> bool {
        match r {
            Reach::Walk => true,
            Reach::Jump => self.jump,
            Reach::Drop => self.drop,
            Reach::Lift => self.lift,
            Reach::Portal => self.portal,
        }
    }
}

/// One place worth standing.
#[derive(Clone, Debug)]
pub struct Node {
    pub at: Vec3,
    /// How enclosed it is, 0 open floor and 1 a corner to hide in. Nothing
    /// reads it yet; it is what a retreating or sniping bot would want.
    pub cover: f32,
    /// A name a decision layer can ask for ("rocket", "gantry").
    pub poi: Option<String>,
    /// Damage a second for standing here (`matlib::hurt`): the slag in a pit,
    /// the lava in an imported map. Resolved against the real voxels when the
    /// graph is built, so no world lists its own hazards and a node that
    /// stops being molten stops being marked.
    pub hurts: bool,
}

#[derive(Clone, Copy, Debug)]
struct Edge {
    to: u16,
    reach: Reach,
    cost: f32,
}

/// The graph. `adj` is indexed by node.
#[derive(Default)]
pub struct Graph {
    pub nodes: Vec<Node>,
    adj: Vec<Vec<Edge>>,
    /// Walk links the world states but the ground refuses: somewhere along
    /// the straight line between the two nodes there is no floor within a
    /// metre, or the floor burns (`unsafe_walk`). Left out of the graph and
    /// kept here for the headless report.
    pub refused: Vec<(u16, u16)>,
}

/// How far apart the checks along a walk link are, metres.
const WALK_CHECK_M: f32 = 0.4;
/// How far the floor under a walk link may fall below the line between its
/// ends before the link counts as crossing a hole: a step, not a pit.
const WALK_GAP_M: f32 = 1.5;

/// Does the straight line from `a` to `b` cross a hole or burning ground?
/// A bot follows a walk link as a straight line, so a link drawn diagonally
/// past the corner of a pit takes every bot that follows it over the edge:
/// on one arena the one link that did so was where nearly every bot that died
/// in the slime fell in. A point whose floor cannot be found yet (the chunk
/// is still loading) is given the benefit of the doubt. Burning floor counts
/// only between two dry nodes (`fire`): a pool's own links are how a bot
/// standing in one plans its way out.
fn unsafe_walk(a: Vec3, b: Vec3, fire: bool, floor: &impl Fn(f32, f32, f32) -> Option<f32>, hurts: &impl Fn(Vec3) -> bool) -> bool {
    let len = Vec3::new(b.x - a.x, 0.0, b.z - a.z).length();
    let n = (len / WALK_CHECK_M).ceil() as usize;
    (1..n).any(|k| {
        let p = a.lerp(b, k as f32 / n as f32);
        match floor(p.x, p.z, p.y + FLOOR_SLACK) {
            Some(y) => y < p.y - WALK_GAP_M || (fire && hurts(Vec3::new(p.x, y, p.z))),
            None => false,
        }
    })
}

/// What waiting for a lift is worth in metres of walking. Not tuned against
/// anything yet: it is the length of an unhurried walk across a room, which
/// is about what standing on a plate feels like.
const LIFT_WAIT_M: f32 = 14.0;
/// And a portal, which is a step through a door however far the far end is.
const PORTAL_M: f32 = 3.0;

/// Where a node's floor is looked for, relative to the node's own height:
/// a node says how high up it is, and the floor is the solid surface just
/// under that.
const FLOOR_SLACK: f32 = 1.6;

impl Graph {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Build the graph from a world's `nav` block. `base` is the floor the
    /// world's own spawn point stands on, which is what every node's `up` is
    /// measured from: a world does not know what height its deck ended up
    /// at, but it does know a walkway is four metres over the floor.
    ///
    /// `floor` finds the solid surface below a point, so a node lands on the
    /// real geometry rather than where the script guessed. Nodes whose floor
    /// cannot be found keep their stated height and are reported.
    /// `hurts` says whether standing at a point costs health, which routing
    /// then avoids: a path that crosses a slag pool is a path that kills the
    /// bot walking it. Measured before this existed: an easy bot on Slagdeck
    /// died three times in a two-minute match with nobody shooting at it,
    /// because the graph offered the pit floor as a place to walk through and
    /// it took the offer. Refusing to *step* into the stuff is not enough on
    /// its own, since a bot reaches the pit floor by dropping into it.
    pub fn build(def: &NavDef, base: f32, floor: impl Fn(f32, f32, f32) -> Option<f32>, hurts: impl Fn(Vec3) -> bool, mut warn: impl FnMut(String)) -> Graph {
        let mut nodes: Vec<Node> = Vec::with_capacity(def.nodes.len());
        for (i, n) in def.nodes.iter().enumerate() {
            let want = base + n.up;
            let y = match floor(n.at[0], n.at[1], want + FLOOR_SLACK) {
                Some(y) => y,
                None => {
                    warn(format!("nav node {i} at {:?} has no floor under it", n.at));
                    want
                }
            };
            if (y - want).abs() > FLOOR_SLACK {
                warn(format!("nav node {i} at {:?} wanted {want:.1} m and found floor at {y:.1} m", n.at));
            }
            let at = Vec3::new(n.at[0], y, n.at[1]);
            nodes.push(Node {
                at,
                cover: n.cover,
                poi: n.poi.clone(),
                hurts: hurts(at),
            });
        }
        let mut adj = vec![Vec::new(); nodes.len()];
        let mut refused = Vec::new();
        for (i, l) in def.links.iter().enumerate() {
            let (a, b) = (l.a as usize, l.b as usize);
            if a >= nodes.len() || b >= nodes.len() || a == b {
                warn(format!("nav link {i} joins {a} to {b}, which is not a pair of nodes"));
                continue;
            }
            let Some(reach) = Reach::parse(&l.reach) else {
                warn(format!("nav link {i}: no reach called {}", l.reach));
                continue;
            };
            let dry = !nodes[a].hurts && !nodes[b].hurts;
            if reach == Reach::Walk && unsafe_walk(nodes[a].at, nodes[b].at, dry, &floor, &hurts) {
                refused.push((a as u16, b as u16));
                continue;
            }
            let d = nodes[a].at.distance(nodes[b].at);
            // What it costs to use, which for a lift is mostly not distance.
            // A plate ten metres below its landing is ten metres of travel
            // and several seconds of standing still waiting for it, and a
            // router that prices it at ten will send a bot up and down a
            // lift to save a short walk. A portal is the opposite: you step
            // in and you are there, so it is worth about the walk to the
            // door and nothing beyond it.
            let cost = if l.cost > 0.0 {
                l.cost
            } else {
                match reach {
                    Reach::Lift => d + LIFT_WAIT_M,
                    Reach::Portal => PORTAL_M,
                    _ => d,
                }
            };
            adj[a].push(Edge { to: b as u16, reach, cost });
            // A drop goes down only, and so does a portal: the far end
            // sends you back only if it is itself a portal, and the world
            // file says so by stating that link too.
            if !matches!(reach, Reach::Drop | Reach::Portal) {
                adj[b].push(Edge { to: a as u16, reach, cost });
            }
        }
        // If every node in the world burns, the marking is wrong rather than
        // the world: routing refuses to enter hurting ground, so a graph
        // marked end to end has no routes in it at all and every bot stands
        // still. A real hazard is a pool in a room, not the floor of the
        // whole map, so take the whole-graph case as a bad reading and drop
        // it rather than hand the brain a map it cannot use.
        if !nodes.is_empty() && nodes.iter().all(|n| n.hurts) {
            warn("every node stands on ground that hurts, which cannot be right: ignoring it".into());
            for n in &mut nodes {
                n.hurts = false;
            }
        }
        Graph { nodes, adj, refused }
    }

    /// The node nearest `p`, preferring one on roughly the same level: the
    /// node directly below a walkway is three metres away in a straight line
    /// and a long walk in fact.
    pub fn nearest(&self, p: Vec3) -> Option<u16> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| !n.hurts)
            .map(|(i, n)| {
                let flat = Vec3::new(n.at.x - p.x, 0.0, n.at.z - p.z).length();
                // Three metres of height costs as much as ten of floor.
                (i as u16, flat + (n.at.y - p.y).abs() * 3.2)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// The node nearest `p` that this body can also see, so a bot does not
    /// set off toward a node on the far side of a wall it is standing
    /// against. Falls back to `nearest`.
    pub fn nearest_seen(&self, store: &crate::world::store::Store, from: Vec3, p: Vec3) -> Option<u16> {
        self.nearest_seen_except(store, from, p, |_| false)
    }

    /// The same, passing over the nodes `skip` names: a node a bot has just
    /// failed to walk onto from where it stands is no place to start from.
    pub fn nearest_seen_except(&self, store: &crate::world::store::Store, from: Vec3, p: Vec3, skip: impl Fn(u16) -> bool) -> Option<u16> {
        let mut best: Option<(u16, f32)> = None;
        for (i, n) in self.nodes.iter().enumerate() {
            if n.hurts || skip(i as u16) {
                continue;
            }
            let flat = Vec3::new(n.at.x - p.x, 0.0, n.at.z - p.z).length();
            let d = flat + (n.at.y - p.y).abs() * 3.2;
            if best.is_some_and(|(_, bd)| d >= bd) {
                continue;
            }
            if crate::combat::shot::sees(store, from, n.at + Vec3::Y * 0.6) {
                best = Some((i as u16, d));
            }
        }
        best.map(|(i, _)| i).or_else(|| self.nearest(p))
    }

    /// A\* from `from` to `to` over the links this body can use. The
    /// straight-line distance is an admissible heuristic because no link
    /// costs less than its own length.
    pub fn route(&self, from: u16, to: u16, able: Able) -> Option<Vec<u16>> {
        self.route_with(from, to, able, |_, _| 0.0)
    }

    /// The same, with `extra(a, b)` metres added to the link from `a` to
    /// `b`: how a bot steers its routes off a link it has just failed to
    /// walk, without ruling it out when there is no other way.
    pub fn route_with(&self, from: u16, to: u16, able: Able, extra: impl Fn(u16, u16) -> f32) -> Option<Vec<u16>> {
        let n = self.nodes.len();
        if from as usize >= n || to as usize >= n {
            return None;
        }
        if from == to {
            return Some(vec![from]);
        }
        let goal = self.nodes[to as usize].at;
        let mut best = vec![f32::INFINITY; n];
        let mut came = vec![u16::MAX; n];
        let mut heap: BinaryHeap<Step> = BinaryHeap::new();
        best[from as usize] = 0.0;
        heap.push(Step {
            at: from,
            f: self.nodes[from as usize].at.distance(goal),
        });
        while let Some(Step { at, .. }) = heap.pop() {
            if at == to {
                let mut path = vec![to];
                let mut cur = to;
                while cur != from {
                    cur = came[cur as usize];
                    if cur == u16::MAX {
                        return None;
                    }
                    path.push(cur);
                }
                path.reverse();
                return Some(path);
            }
            let here = best[at as usize];
            for e in &self.adj[at as usize] {
                if !able.can(e.reach) {
                    continue;
                }
                // Never route into ground that burns from ground that does
                // not. Crossing it is allowed only once you are already in
                // it, which is how a bot walks out of a pool it fell into:
                // a pool is several nodes across, so letting it leave only
                // the node it stands on stranded it in the middle. Measured
                // on an imported map: a bot in the slime could reach one node of the
                // 258 and had no route anywhere.
                if self.nodes[e.to as usize].hurts && !self.nodes[at as usize].hurts {
                    continue;
                }
                let g = here + e.cost + extra(at, e.to);
                if g + 1e-4 >= best[e.to as usize] {
                    continue;
                }
                best[e.to as usize] = g;
                came[e.to as usize] = at;
                heap.push(Step {
                    at: e.to,
                    f: g + self.nodes[e.to as usize].at.distance(goal),
                });
            }
        }
        None
    }

    /// Which nodes can be walked to from `from`, as a flood fill over the
    /// links this body is willing to take.
    ///
    /// One pass over the graph, against one A* per node for the same answer,
    /// which is what makes it cheap enough for a bot to keep current. It is
    /// worth keeping current because the alternative is finding out by
    /// failing: a bot choosing from the whole graph on an imported map spends
    /// its decisions asking for routes into parts only a lift reaches, or
    /// into another component entirely, and stands still through every one.
    /// Measured on an imported map before this existed: 64 of an easy bot's 107
    /// route requests in two minutes could never have succeeded.
    pub fn reachable_from(&self, from: u16, able: Able, out: &mut Vec<bool>) {
        out.clear();
        out.resize(self.nodes.len(), false);
        if from as usize >= self.nodes.len() {
            return;
        }
        out[from as usize] = true;
        let mut stack = vec![from];
        while let Some(at) = stack.pop() {
            for e in &self.adj[at as usize] {
                if !able.can(e.reach) || out[e.to as usize] {
                    continue;
                }
                // The same rule `route` follows: into the fire only from
                // the fire, or a body in a pool can reach nothing.
                if self.nodes[e.to as usize].hurts && !self.nodes[at as usize].hurts {
                    continue;
                }
                out[e.to as usize] = true;
                stack.push(e.to);
            }
        }
    }

    pub fn at(&self, i: u16) -> Option<&Node> {
        self.nodes.get(i as usize)
    }

    /// What is wrong with this graph, from a body standing at `from`: the
    /// nodes that burn, the ones it can never get to, and the ones it could
    /// get to but never leave.
    ///
    /// The last are the ones that matter and the ones nothing else notices.
    /// A hand-drawn arena has none; one imported map has two pockets of a
    /// dozen nodes apiece, reached by a drop or a teleporter and left by
    /// neither, and a bot that wanders into one spends the rest of the match
    /// asking for routes that cannot exist -- 78 refusals in two minutes,
    /// and a twenty-second blacklist on a good node for each one.
    pub fn audit(&self, from: u16, able: Able) -> (Vec<u16>, Vec<u16>, Vec<u16>) {
        let hurts = (0..self.nodes.len() as u16).filter(|&i| self.nodes[i as usize].hurts).collect();
        let mut out = Vec::new();
        self.reachable_from(from, able, &mut out);
        let marooned: Vec<u16> = (0..self.nodes.len() as u16).filter(|&i| !out[i as usize]).collect();
        let trapped = (0..self.nodes.len() as u16).filter(|&i| out[i as usize] && i != from && self.route(i, from, able).is_none()).collect();
        (hurts, marooned, trapped)
    }

    /// Is this a node a bot may be sent to? Ground that burns is not, and
    /// every chooser has to agree with `route` about that or a bot spends
    /// its decisions asking for somewhere it cannot be routed to.
    pub fn safe(&self, i: u16) -> bool {
        self.nodes.get(i as usize).is_some_and(|n| !n.hurts)
    }

    /// How a link to this node must be travelled, coming from `prev`.
    pub fn reach_between(&self, prev: u16, next: u16) -> Reach {
        self.adj.get(prev as usize).and_then(|es| es.iter().find(|e| e.to == next)).map_or(Reach::Walk, |e| e.reach)
    }
}

/// A heap entry. `Ord` is reversed so `BinaryHeap` pops the smallest f.
struct Step {
    at: u16,
    f: f32,
}

impl PartialEq for Step {
    fn eq(&self, o: &Self) -> bool {
        self.f == o.f
    }
}
impl Eq for Step {}
impl Ord for Step {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        o.f.total_cmp(&self.f)
    }
}
impl PartialOrd for Step {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

/// How near a node counts as arrived, metres across the floor.
const ARRIVED: f32 = 1.8;
/// And how near the mouth of a portal, which is a thing to step into rather
/// than walk up to: a body that stops 1.8 m short of the node is outside the
/// cylinder that does the sending (`portals::PortalDef::radius`, about a
/// metre), so the cursor moves on, the bot walks off toward the far end
/// through whatever is in the way, and the teleporter is never used at all.
/// Measured on an imported map: its one teleporter was used 0 times in a two-minute
/// match that routed bots past its mouth.
const MOUTH: f32 = 0.5;
/// How near the tail counts as arrived. Under the pickup reach (1.3 m in
/// `shooter::pickup`), because the whole point of the tail is to finish the
/// walk on the thing rather than near it.
const TAIL: f32 = 0.7;
/// How far ahead on the route a body may be found standing. One step is the
/// far end of a portal; the rest is a body that walked through two nodes in
/// a step. Bounded so a route that doubles back past itself cannot be
/// declared finished from the wrong end of it.
const SKIP: usize = 2;

/// Has a body at `pos` arrived at `at`, within `near` metres across the
/// floor? Height is forgiven: a body standing on a crate over a node has
/// arrived.
fn stands_at(at: Vec3, pos: Vec3, near: f32) -> bool {
    Vec3::new(at.x - pos.x, 0.0, at.z - pos.z).length() <= near && (at.y - pos.y).abs() <= 2.5
}

/// Walking a route: which node is next, and when to move on.
#[derive(Default)]
pub struct Path {
    pub steps: Vec<u16>,
    pub at: usize,
    /// A point off the graph to finish on, once the nodes are walked.
    ///
    /// The graph is sparse on purpose and an imported map's pads were not
    /// laid on it: on one, the median pad stands 2.5 m from the nearest
    /// nav node and only 17 of the 66 are inside the 1.3 m it takes to pick
    /// one up. A route to the node is therefore a route to somewhere near
    /// the thing, and a bot that walks it arrives, finds itself holding
    /// nothing, re-paths to the node it is standing on and does that until
    /// the fetch clock gives up. Measured before this existed: three bots
    /// set off for 52 pads in two minutes and picked up 4 between them.
    pub tail: Option<Vec3>,
}

impl Path {
    pub fn new(steps: Vec<u16>) -> Path {
        Path { steps, at: 0, tail: None }
    }

    /// The same route, finishing with a short walk to a point the graph does
    /// not have a node for.
    pub fn to_point(steps: Vec<u16>, at: Vec3) -> Path {
        Path { steps, at: 0, tail: Some(at) }
    }

    pub fn done(&self) -> bool {
        self.at >= self.steps.len() && self.tail.is_none()
    }

    /// Where it is still going, or `None` once it has got there.
    ///
    /// "Still going" is the only useful reading and the other one is a trap.
    /// Every caller asks `goal() != Some(x)` to mean "am I already heading
    /// for x", and answering with the last step of a finished path means a
    /// bot that has arrived says yes for ever: it does not re-path, the
    /// cursor is past the end so there is nothing to walk toward, and it
    /// stands on the spot it reached until the match ends. Measured on an
    /// imported map before this was fixed: every bot but the one with fetching
    /// switched off asked for exactly one route in ninety seconds and spent
    /// 97% of the match motionless.
    pub fn goal(&self) -> Option<u16> {
        if self.done() {
            return None;
        }
        self.steps.last().copied()
    }

    /// Where to head for now: the next node, or the tail once they are all
    /// walked.
    pub fn target(&self, g: &Graph) -> Option<Vec3> {
        match self.steps.get(self.at) {
            Some(&i) => g.at(i).map(|n| n.at),
            None => self.tail,
        }
    }

    /// The reach of the step being taken, which is how the brain knows to
    /// jump.
    pub fn reach(&self, g: &Graph) -> Reach {
        match (self.at.checked_sub(1).and_then(|i| self.steps.get(i)), self.steps.get(self.at)) {
            (Some(&prev), Some(&next)) => g.reach_between(prev, next),
            _ => Reach::Walk,
        }
    }

    /// Move the cursor on if the body has arrived at the current node.
    ///
    /// It looks a step or two ahead as well as at the step it is on, which
    /// is how a body that has just come out of a portal picks the route up
    /// at the far end instead of walking all the way back to the mouth it
    /// went in at.
    pub fn advance(&mut self, g: &Graph, pos: Vec3) {
        let last = (self.at + SKIP).min(self.steps.len().saturating_sub(1));
        for i in (self.at..=last).rev() {
            let Some(&n) = self.steps.get(i) else { continue };
            let Some(node) = g.at(n) else { continue };
            if stands_at(node.at, pos, self.near(g, i)) {
                self.at = i + 1;
                break;
            }
        }
        if self.at >= self.steps.len() {
            if let Some(t) = self.tail {
                if stands_at(t, pos, TAIL) {
                    self.tail = None;
                }
            }
        }
    }

    /// How near the step at `i` counts as arrived. A node you leave through
    /// a portal has to be stood in, not approached.
    fn near(&self, g: &Graph, i: usize) -> f32 {
        match (self.steps.get(i), self.steps.get(i + 1)) {
            (Some(&a), Some(&b)) if g.reach_between(a, b) == Reach::Portal => MOUTH,
            _ => ARRIVED,
        }
    }

    pub fn clear(&mut self) {
        self.steps.clear();
        self.at = 0;
        self.tail = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::fields::{NavLink, NavNode};

    fn node(x: f32, z: f32, up: f32) -> NavNode {
        NavNode {
            at: [x, z],
            up,
            cover: 0.0,
            poi: None,
        }
    }

    fn link(a: u16, b: u16, reach: &str) -> NavLink {
        NavLink {
            a,
            b,
            reach: reach.to_string(),
            cost: 0.0,
            platform: None,
            portal: None,
        }
    }

    /// Four nodes in a line, the last reachable only by a jump: a bot that
    /// can jump gets there and one that cannot does not.
    fn ladder() -> Graph {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(10.0, 0.0, 0.0), node(20.0, 0.0, 0.0), node(20.0, 10.0, 1.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "walk"), link(2, 3, "jump")],
        };
        Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"))
    }

    /// A walk link drawn across the corner of a pit is left out, and so is
    /// one across burning floor; a jump over the same pit is kept.
    #[test]
    fn a_walk_link_over_a_hole_or_fire_is_refused() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(10.0, 0.0, 0.0), node(0.0, 10.0, 0.0), node(10.0, 10.0, 0.0)],
            links: vec![link(0, 1, "walk"), link(2, 3, "walk"), link(0, 2, "walk"), link(1, 3, "jump")],
        };
        // A pit 2.5 m deep across x 4..6 at z < 5, and fire across x 4..6 at z > 5.
        let floor = |x: f32, _z: f32, want: f32| Some(if (4.0..6.0).contains(&x) { want - FLOOR_SLACK - 2.5 } else { want - FLOOR_SLACK });
        let hurts = |p: Vec3| (4.0..6.0).contains(&p.x) && p.z > 5.0;
        let g = Graph::build(&def, 0.0, floor, hurts, |w| panic!("{w}"));
        assert_eq!(g.refused, vec![(0, 1), (2, 3)]);
        let able = Able::default();
        assert!(g.route(0, 2, able).is_some(), "the safe walk went too");
        assert!(g.route(1, 3, able).is_some(), "the jump went too");
    }

    #[test]
    fn a_link_says_what_it_takes_and_the_search_listens() {
        let g = ladder();
        assert_eq!(g.len(), 4);
        let can = Able::default();
        let cannot = Able {
            jump: false,
            drop: false,
            ..Able::default()
        };
        assert_eq!(g.route(0, 3, can).map(|p| p.len()), Some(4));
        assert!(g.route(0, 3, cannot).is_none(), "it planned a jump it cannot make");
        assert_eq!(g.route(0, 2, cannot).map(|p| p.len()), Some(3), "it refused a walk");
        assert_eq!(g.route(2, 2, can), Some(vec![2]));
        assert_eq!(g.audit(0, cannot).1, vec![3]);
        assert!(g.audit(0, can).1.is_empty());
    }

    /// A drop is one way: you can fall off the gantry and not climb back.
    #[test]
    fn a_drop_only_goes_down() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 6.0), node(6.0, 0.0, 0.0)],
            links: vec![link(0, 1, "drop")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let able = Able::default();
        assert!(g.route(0, 1, able).is_some(), "it would not step off");
        assert!(g.route(1, 0, able).is_none(), "it climbed up a fall");
    }

    /// The search takes the short way round when there is one.
    #[test]
    fn it_takes_the_short_way() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(40.0, 0.0, 0.0), node(80.0, 0.0, 0.0), node(40.0, 60.0, 0.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "walk"), link(0, 3, "walk"), link(3, 2, "walk")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        assert_eq!(g.route(0, 2, Able::default()), Some(vec![0, 1, 2]));
    }

    /// Walking a path moves the cursor on as the body arrives, and the
    /// nearest node prefers one on the same level.
    #[test]
    fn walking_a_path_and_finding_the_near_node() {
        let g = ladder();
        let mut p = Path::new(g.route(0, 2, Able::default()).unwrap());
        assert_eq!(p.target(&g).map(|v| v.x), Some(0.0));
        p.advance(&g, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(p.target(&g).map(|v| v.x), Some(10.0));
        p.advance(&g, Vec3::new(10.2, 0.0, 0.0));
        assert_eq!(p.target(&g).map(|v| v.x), Some(20.0));
        p.advance(&g, Vec3::new(20.0, 0.0, 0.0));
        assert!(p.done());
    }

    /// Two nodes over the same spot, one on the floor and one on a walkway
    /// four metres up: a body on the floor is nearest the floor one, even
    /// though the other is directly overhead.
    #[test]
    fn the_near_node_is_one_on_your_own_level() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 4.0), node(3.0, 0.0, 0.0)],
            links: vec![link(0, 1, "drop")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        assert_eq!(g.nearest(Vec3::new(0.0, 0.0, 0.0)), Some(1), "it reached up to the walkway");
        assert_eq!(g.nearest(Vec3::new(0.0, 4.0, 0.0)), Some(0), "it reached down off the walkway");
    }

    /// A lift is a link you may take and a cost you would rather not pay,
    /// and a portal goes one way.
    #[test]
    fn a_lift_is_a_wait_and_a_portal_is_a_door() {
        // Two floors. The long way round is 0-1-2 on foot; the short way is
        // the lift from 0 straight up to 2.
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(9.0, 0.0, 0.0), node(0.0, 9.0, 0.0), node(40.0, 0.0, 0.0), node(80.0, 0.0, 0.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "walk"), link(0, 2, "lift"), link(3, 4, "portal")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let can = Able::default();
        // It will use the lift, and both ways: unlike a drop, a plate goes
        // back down.
        assert!(g.route(0, 2, can).is_some());
        assert!(g.route(2, 0, can).is_some(), "it could ride up but not down");
        // A bot that has sworn off lifts walks round instead of standing.
        let walking = Able { lift: false, ..Able::default() };
        assert_eq!(g.route(0, 2, walking).map(|p| p.len()), Some(3), "it refused to walk round a lift it would not take");
        // And the wait is priced in: a nine-metre ride costs more than an
        // eighteen-metre walk, or a bot takes the lift to save three steps.
        assert_eq!(g.route(0, 2, can).map(|p| p.len()), Some(3), "it rode a lift to save a short walk");
        // The portal is one way, as the world file states it.
        assert!(g.route(3, 4, can).is_some());
        assert!(g.route(4, 3, can).is_none(), "a portal sent a bot back through itself");
        // And a portal is cheap, so it beats walking the long way round
        // when both exist.
        assert!(g.reach_between(3, 4) == Reach::Portal);
    }

    /// A route may finish off the graph: the last few metres to a thing
    /// standing between nodes, which is where an imported map's pickups are.
    #[test]
    fn a_path_can_finish_on_a_point_the_graph_has_no_node_for() {
        let g = ladder();
        let pad = Vec3::new(22.5, 0.0, 0.0);
        let mut p = Path::to_point(g.route(0, 2, Able::default()).unwrap(), pad);
        // Standing on the last node is not arriving any more.
        p.advance(&g, Vec3::new(0.0, 0.0, 0.0));
        p.advance(&g, Vec3::new(10.0, 0.0, 0.0));
        p.advance(&g, Vec3::new(20.0, 0.0, 0.0));
        assert!(!p.done(), "it stopped at the node and called the pad fetched");
        assert_eq!(p.target(&g), Some(pad), "it was not walking at the pad");
        // And walking the last couple of metres finishes it.
        p.advance(&g, Vec3::new(22.2, 0.0, 0.0));
        assert!(p.done());
        assert_eq!(p.target(&g), None);
    }

    /// A portal is a thing to step into. The cursor must not move past the
    /// mouth because the body got within the ordinary arriving distance of
    /// it, or the bot walks off toward the far end on foot and never
    /// teleports -- and once it has been sent, the route picks up at the far
    /// end rather than walking back.
    #[test]
    fn a_portal_mouth_has_to_be_stepped_into() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(8.0, 0.0, 0.0), node(60.0, 0.0, 0.0), node(68.0, 0.0, 0.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "portal"), link(2, 3, "walk")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let mut p = Path::new(g.route(0, 3, Able::default()).expect("no route through the portal"));
        p.advance(&g, Vec3::new(0.0, 0.0, 0.0));
        assert_eq!(p.target(&g).map(|t| t.x), Some(8.0), "it was not heading for the mouth");
        // A metre short of the mouth is outside the cylinder that does the
        // sending, so it is still walking at it.
        p.advance(&g, Vec3::new(7.0, 0.0, 0.0));
        assert_eq!(p.target(&g).map(|t| t.x), Some(8.0), "it gave up on the mouth a metre short");
        // Standing in it, and then sent: the body is at the far end, and the
        // route goes on from there.
        p.advance(&g, Vec3::new(8.2, 0.0, 0.0));
        p.advance(&g, Vec3::new(60.0, 0.0, 0.0));
        assert_eq!(p.target(&g).map(|t| t.x), Some(68.0), "it walked back to the teleporter it came out of");
    }

    /// The audit finds the pocket a body can get into and not out of, which
    /// is the one fault in a graph that nothing else notices: every route
    /// out of it fails, and a bot in one spends the match asking.
    #[test]
    fn the_audit_finds_a_one_way_pocket() {
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(8.0, 0.0, 0.0), node(16.0, 0.0, 4.0), node(90.0, 0.0, 0.0)],
            // 0-1 walk, 1 drops to 2 and cannot climb back, 3 is on its own.
            links: vec![link(0, 1, "walk"), link(2, 1, "drop")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let (hurts, marooned, trapped) = g.audit(0, Able::default());
        assert!(hurts.is_empty());
        assert_eq!(marooned, vec![2, 3], "it did not notice the parts it cannot reach");
        assert!(trapped.is_empty(), "nothing it can reach is a trap yet");
        // Now give it a way in and no way out: that is the trap.
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(8.0, 0.0, 0.0), node(16.0, 0.0, 4.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "drop")],
        };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let (_, marooned, trapped) = g.audit(0, Able::default());
        assert!(marooned.is_empty());
        assert_eq!(trapped, vec![2], "it sent bots somewhere they could never leave and called it fine");
    }

    /// A pool is several nodes across, so a body in the middle of one has
    /// to be able to plan its way out across the rest of it -- while
    /// nothing on dry ground is ever routed in.
    #[test]
    fn it_can_walk_out_of_a_pool_but_never_into_one() {
        // 0 dry, 1 and 2 molten, 3 dry: a line.
        let def = NavDef {
            nodes: vec![node(0.0, 0.0, 0.0), node(6.0, 0.0, 0.0), node(12.0, 0.0, 0.0), node(18.0, 0.0, 0.0)],
            links: vec![link(0, 1, "walk"), link(1, 2, "walk"), link(2, 3, "walk")],
        };
        let hot = |p: Vec3| p.x > 3.0 && p.x < 15.0;
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), hot, |w| panic!("{w}"));
        assert!(g.at(1).unwrap().hurts && g.at(2).unwrap().hurts, "the pool was not marked");
        let able = Able::default();
        assert!(g.route(1, 3, able).is_some(), "a bot in the pool had no way out of it");
        assert!(g.route(0, 3, able).is_none(), "it routed a bot across the pool");
        assert!(g.route(0, 1, able).is_none(), "it routed a bot into the pool");
        let mut out = Vec::new();
        // Out either way: the near bank and the far one.
        g.reachable_from(2, able, &mut out);
        assert_eq!(out.iter().filter(|&&r| r).count(), 4, "a bot standing in it could reach {out:?}");
        g.reachable_from(0, able, &mut out);
        assert_eq!(out.iter().filter(|&&r| r).count(), 1, "dry ground could reach into the pool");
    }

    /// What the reachability flood costs, since a bot runs one every four
    /// seconds and the imported maps raised the node cap to 400 to make
    /// their lift landings join up.
    ///
    /// Asserted rather than printed because the number is the point: it is
    /// one pass over the links, so it has to stay in the microseconds, and
    /// if it ever does not then the four-second refresh is the thing that
    /// has to change rather than the maps.
    #[test]
    fn a_reachability_flood_is_cheap_enough_to_keep_current() {
        // Four hundred nodes in a chain with cross-links: more edges than
        // either imported map carries.
        let n = 400u16;
        let nodes: Vec<NavNode> = (0..n).map(|i| node(i as f32 * 3.0, 0.0, 0.0)).collect();
        let mut links = Vec::new();
        for i in 0..n - 1 {
            links.push(link(i, i + 1, "walk"));
            if i + 7 < n {
                links.push(link(i, i + 7, "walk"));
            }
        }
        let def = NavDef { nodes, links };
        let g = Graph::build(&def, 0.0, |_, _, want| Some(want - FLOOR_SLACK), |_| false, |w| panic!("{w}"));
        let mut out = Vec::new();
        let began = std::time::Instant::now();
        for _ in 0..100 {
            g.reachable_from(0, Able::default(), &mut out);
        }
        let each = began.elapsed().as_secs_f64() / 100.0;
        assert!(out.iter().all(|&r| r), "the chain was not all reachable");
        assert!(each < 0.001, "a flood over {n} nodes took {:.3} ms, which is too much to run every four seconds", each * 1000.0);
    }
}
