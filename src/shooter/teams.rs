//! Two sides: who is on which, and what they look like.
//!
//! A match is a team match when the world's rules say so (`teams`, or a
//! `ctf` block). Everything else about teams is one rule applied in a few
//! places in `mod.rs`: a teammate is not a target, a teammate's round does
//! `friendly_fire` of its damage (and all of its push; in full from a weapon
//! that can hurt its own firer), a teammate's death is
//! nobody's kill, and a body comes back at its own side's spawns.

use crate::world::fields::{BotDef, ShooterDef};

/// Whether these rules play two sides.
pub fn active(rules: &ShooterDef) -> bool {
    rules.teams || rules.ctf.is_some()
}

/// Deal the sides: the player's, and each bot's in order. A bot the world
/// put on a side stays there; the rest go to whichever side is shorter, so
/// a roster of five with the player makes three against three. The player
/// joins the side the launch screen named, or the shorter of the named ones.
pub fn deal(bots: &[BotDef], player: Option<u8>) -> (u8, Vec<u8>) {
    let mut count = [0usize; 2];
    for b in bots {
        if let Some(t) = b.team {
            count[(t as usize).min(1)] += 1;
        }
    }
    let me = player.map(|t| t.min(1)).unwrap_or(if count[1] < count[0] { 1 } else { 0 });
    count[me as usize] += 1;
    let mut out = Vec::with_capacity(bots.len());
    for b in bots {
        let t = match b.team {
            Some(t) => t.min(1),
            None => {
                let t = if count[1] < count[0] { 1 } else { 0 };
                count[t as usize] += 1;
                t
            }
        };
        out.push(t);
    }
    (me, out)
}

/// A side's plate colour, lightened or darkened a little for the `k`th
/// member so two bots on a side can still be told apart close up. The two
/// bases are the first and third of `avatar::TEAM_COLOURS`: cold cyan and
/// magenta. The chest lamps take the base, unshaded, so a side's lamps all
/// match.
pub fn tint(team: u8, k: usize) -> [f32; 3] {
    let base = crate::brain::avatar::team_tint(if team == 0 { 0 } else { 2 });
    let shade = [1.0, 0.82, 1.12, 0.9, 1.05, 0.76][k % 6];
    base.map(|c| (c * shade).min(1.0))
}

/// What `--validate` says about a world's capture-the-flag layout, read off
/// the rules as written (the nav graph at its stated heights):
///
/// - a flag for each of the two sides, and each within reach of a nav node,
///   so a bot can route to it;
/// - at least two spawns marked for each side;
/// - from each side's spawns a route to the other side's stand, and from
///   there a route back to its own: a one-way drop that strands a carrier
///   is the fault this exists to catch;
/// - and the two sides' trips about the same length, since a match where
///   one side's run is shorter is decided by the map.
pub fn check(rules: &ShooterDef) -> Vec<String> {
    use crate::brain::nav::{Able, Graph};
    use glam::Vec3;
    let mut out = Vec::new();
    let Some(ctf) = &rules.ctf else { return out };
    let graph = Graph::build(&rules.nav, 0.0, |_, _, _| None, |_| false, |_| {});
    // The nearest node, and how far it is across the floor. With a height
    // (graph metres, from the first spawn's floor) the node is chosen on
    // that floor: in plan alone, a stand under a tower picks a node on the
    // tower's top (Starfall, where that made one side's run 76 m longer).
    let node_at = |x: f32, z: f32, up: Option<f32>| -> Option<(u16, f32)> {
        let flat = |i: u16| graph.at(i).map_or(f32::MAX, |n| Vec3::new(n.at.x - x, 0.0, n.at.z - z).length());
        let d = |i: u16| match (up, graph.at(i)) {
            (Some(y), Some(n)) => n.at.distance(Vec3::new(x, y, z)),
            _ => flat(i),
        };
        let i = (0..graph.len() as u16).min_by(|&a, &b| d(a).total_cmp(&d(b)))?;
        Some((i, flat(i)))
    };
    let first = rules.spawns.first().and_then(|s| s.ceiling);
    let spawn_up = |s: &crate::world::fields::SpawnPoint| s.ceiling.zip(first).map(|(c, c0)| c - c0);
    let length = |steps: &[u16]| -> f32 { steps.windows(2).filter_map(|w| Some(graph.at(w[0])?.at.distance(graph.at(w[1])?.at))).sum() };
    let mut stands = [None, None];
    for t in 0..2u8 {
        let flags: Vec<_> = ctf.flags.iter().filter(|f| f.team == t).collect();
        if flags.len() != 1 {
            out.push(format!("ctf: {} flags for team {t}, and it wants one", flags.len()));
            continue;
        }
        match node_at(flags[0].at[0], flags[0].at[1], flags[0].up) {
            Some((i, d)) if d <= 3.0 => stands[t as usize] = Some(i),
            Some((_, d)) => out.push(format!("ctf: team {t}'s flag is {d:.1} m from the nearest nav node; a bot cannot route to it")),
            None => out.push("ctf: there is no nav graph, so no bot can play".to_string()),
        }
        if rules.spawns.iter().filter(|s| s.team == Some(t)).count() < 2 {
            out.push(format!("ctf: fewer than two spawns marked for team {t}"));
        }
    }
    let [Some(s0), Some(s1)] = stands else { return out };
    let mut trip = [0.0f32; 2];
    for t in 0..2u8 {
        let (own, theirs) = if t == 0 { (s0, s1) } else { (s1, s0) };
        let starts: Vec<u16> = rules.spawns.iter().filter(|s| s.team == Some(t)).filter_map(|s| node_at(s.at[0], s.at[1], spawn_up(s)).map(|(i, _)| i)).collect();
        for &from in &starts {
            match graph.route(from, theirs, Able::default()) {
                Some(r) => trip[t as usize] = trip[t as usize].max(length(&r)),
                None => out.push(format!("ctf: no route from a team {t} spawn (node {from}) to the other flag")),
            }
        }
        match graph.route(theirs, own, Able::default()) {
            Some(r) => trip[t as usize] += length(&r),
            None => out.push(format!("ctf: a team {t} carrier cannot get from the other flag back to its own stand")),
        }
    }
    if trip[0] > 0.0 && trip[1] > 0.0 && (trip[0] - trip[1]).abs() > 0.1 * trip[0].max(trip[1]) {
        out.push(format!("ctf: the sides' runs differ by more than a tenth: {:.0} m and {:.0} m", trip[0], trip[1]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot(team: Option<u8>) -> BotDef {
        BotDef {
            name: "b".into(),
            skill: "average".into(),
            colour: None,
            bold: 0.0,
            steady: 0.0,
            without: Vec::new(),
            tune: Default::default(),
            team,
        }
    }

    #[test]
    fn the_sides_come_out_even() {
        let (me, bots) = deal(&vec![bot(None); 5], None);
        let mut n = [0, 0];
        n[me as usize] += 1;
        for t in &bots {
            n[*t as usize] += 1;
        }
        assert_eq!(n, [3, 3], "five bots and a player dealt {n:?}");
    }

    #[test]
    fn a_side_the_world_named_is_kept() {
        let (me, bots) = deal(&[bot(Some(1)), bot(Some(1)), bot(None)], None);
        assert_eq!(me, 0, "the player went to the longer side");
        assert_eq!(bots, vec![1, 1, 0]);
        let (me, _) = deal(&[bot(None)], Some(1));
        assert_eq!(me, 1, "the launch screen's side was not kept");
    }

    #[test]
    fn the_two_sides_do_not_look_alike() {
        for k in 0..6 {
            let (a, b) = (tint(0, k), tint(1, k));
            let d: f32 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum();
            assert!(d > 0.15, "member {k} of each side are nearly the same colour");
        }
    }
}
