//! Laying what happens on the live grid (world/live.rs). Every animal step
//! the trampled ground goes down as it happens; every tick (`live::TICK_S`)
//! the state of things is read once: the player (danger by how they move,
//! their footsteps, their scent), every walker (counted as grazer or
//! predator, danger where one bolts or hunts, a predator's scent), every
//! carcass, and any shot fired since the last tick. What goes on (the
//! player about, an animal running) is held at a level (`Live::hold`); what
//! happens once (a shot) is stamped and fades.

use super::Game;
use crate::creatures::carcass::Carcass;
use crate::creatures::startle::Threat;
use crate::creatures::walk::{Critter, Mood};
use crate::creatures::{Creature, Look};
use crate::world::live::LiveChan as L;

/// Metres a second the scent drifts per unit of wind carry (`Threat::scent`
/// is the wind's direction times how far it carries, about 1 on a breeze).
const SCENT_DRIFT: f32 = 2.0;

impl Game {
    /// After the animals ran for `dt` seconds: lay what happened, and tick the
    /// grid when one is due.
    pub(super) fn live_step(&mut self, dt: f32, player: &Threat) {
        let live = &mut self.ents.live;
        if live.is_empty() {
            return;
        }
        for &b in &self.ents.crush {
            live.stamp(L::Disturbed, b, 0.3, 4.0);
        }
        if !live.due(dt) {
            return;
        }
        // The player: a person about is a little danger, one running or on
        // an engine more; footsteps and engines are noise; scent where they
        // stand, carried by the tick's drift.
        let speed = player.vel.length();
        live.hold(L::Threat, player.pos, player.menace * (0.15 + 0.5 * (speed / 6.0).min(1.0)), 24.0);
        live.hold(L::Noise, player.pos, (player.noise * 0.4).min(2.0), 12.0 + 10.0 * player.noise.min(3.0));
        live.hold(L::Scent, player.pos, 1.0, 8.0 + 8.0 * player.pool);

        // Shots heard since the last tick, in a match.
        if let Some(sh) = self.activity.as_ref().and_then(|a| a.shooter()) {
            // A new match starts its clock again: forget the old mark.
            if sh.noises().iter().any(|n| n.when + 30.0 < live.heard_to) {
                live.heard_to = 0.0;
            }
            let since = live.heard_to;
            let mut newest = since;
            for n in sh.noises().iter().filter(|n| n.when > since) {
                live.stamp(L::Noise, n.at, 1.0, n.reach);
                live.stamp(L::Threat, n.at, 1.0, n.reach);
                newest = newest.max(n.when);
            }
            live.heard_to = newest;
        }

        let species = &self.ents.species;
        for (look, c, crit) in self.ents.world.query::<(&Look, &Creature, &Critter)>().iter() {
            let hunter = species.get(c.species).is_some_and(|s| !s.hunts.is_empty());
            live.count(if hunter { L::Predators } else { L::Grazers }, look.pos, 1.0);
            match crit.mood {
                // A bolt or a chase lasts seconds: stamped each tick it goes on.
                Mood::Flee => live.stamp(L::Threat, look.pos, 0.25, 16.0),
                Mood::Chase => {
                    live.stamp(L::Threat, look.pos, 0.5, 24.0);
                    live.hold(L::Scent, look.pos, 0.6, 8.0);
                }
                _ if hunter => live.hold(L::Scent, look.pos, 0.3, 8.0),
                _ => {}
            }
        }
        for c in self.ents.world.query::<&Carcass>().iter() {
            live.count(L::Carrion, c.at, if c.bones { 0.2 } else { 1.0 });
        }

        let wind = glam::Vec2::new(player.scent.x, player.scent.z) * SCENT_DRIFT;
        live.tick(wind);
        // A live layer on the open map is drawn from the grid: send it again.
        if matches!(self.eco.layer, super::eco::Layer::Live(_)) {
            self.eco_redraw();
        }
    }
}
