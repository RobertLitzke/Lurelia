use crate::creatures::roost::RoostDef;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use glam::Vec3;
use serde::Deserialize;

/// A roost: a cave mouth or a skylight that empties at dusk
/// (`creatures/roost.rs`). In its hours, with the player near, the named
/// circlers (`circle`, `flutter`, `drift` or `undulate` movers) come up out of the ground at `at`
/// one at a time and spiral out over the land: the bats' evening exodus.
#[derive(Deserialize, Debug)]
pub struct Roost {
    at: Pos,
    /// Creature names; each one let out is picked from these.
    creatures: Vec<String>,
    /// How many come out in an evening.
    #[serde(default = "d_count")]
    count: u32,
    /// Game hours they go out between.
    #[serde(default = "d_hours")]
    hours: [f32; 2],
    /// Metres under the ground at `at` they rise from.
    #[serde(default = "d_depth")]
    depth: f32,
}

fn d_count() -> u32 {
    40
}
fn d_hours() -> [f32; 2] {
    [18.0, 19.2]
}
fn d_depth() -> f32 {
    4.0
}

/// The roosts a world's chronicle made, for `World::roosts`.
#[derive(Default)]
pub struct Roosts(pub Vec<RoostDef>);

impl Process for Roost {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let p = cx.pos(&self.at);
        for c in &self.creatures {
            match cx.lib.creature(c) {
                None => cx.warnings.push(format!("roost: unknown creature {c}")),
                Some(i) => {
                    let mv = cx.lib.creatures[i].behaviour.movement.as_str();
                    if !matches!(mv, "circle" | "flutter" | "drift" | "undulate") {
                        cx.warnings.push(format!("roost: {c} moves by {mv}; only circlers come out of a roost"));
                    }
                }
            }
        }
        let at = Vec3::new(p.x, cx.f.height(p.x, p.y), p.y);
        cx.state.get::<Roosts>().0.push(RoostDef { at, depth: self.depth, species: self.creatures.clone(), count: self.count, hours: self.hours });
        format!("roost at {:.0},{:.0}: {} {} out between {:.1} and {:.1}", p.x, p.y, self.count, self.creatures.join(" and "), self.hours[0], self.hours[1])
    }
}
