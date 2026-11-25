use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::Pos;
use serde::Deserialize;

/// Where another realm's life comes through (`library/realms/`): a relict
/// Mesozoic wood, a faerie ring, ground the skystone changed. Full strength
/// out to `radius` metres from `at`, fading to nothing over `edge` more.
/// Painted onto the site grid's realm overlays when the site fields are
/// built (`world/realm_map.rs`); fill and a world's realm looks read them.
#[derive(Deserialize, Debug)]
pub struct RealmEvent {
    realm: String,
    at: Pos,
    radius: f32,
    #[serde(default)]
    edge: Option<f32>,
    #[serde(default = "one")]
    strength: f32,
}

fn one() -> f32 {
    1.0
}

impl Process for RealmEvent {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let at = cx.pos(&self.at);
        let edge = self.edge.unwrap_or(self.radius * 0.5);
        cx.f.realm_paint.push(crate::world::realm_map::Paint { realm: self.realm.clone(), at, radius: self.radius, edge, strength: self.strength.clamp(0.0, 1.0) });
        format!("{} realm at {:.0},{:.0} out to {:.0} m", self.realm, at.x, at.y, self.radius + edge)
    }
}
