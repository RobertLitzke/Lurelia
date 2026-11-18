use super::settlement::Settlements;
use crate::world::chronicle::{EventCtx, Process};
use serde::Deserialize;

/// A settlement is left: its buildings' age becomes the years since this
/// event, so they decay.
#[derive(Deserialize, Debug)]
pub struct Abandon {
    settlement: String,
}

impl Process for Abandon {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let settlement = &self.settlement;
        let years = (-cx.year) as f32;
        match cx.state.get::<Settlements>().0.get(settlement) {
            Some(list) => {
                for &k in list {
                    cx.placed[k].age = years;
                }
                format!("{settlement} left {years} years ago ({} buildings decay)", list.len())
            }
            None => {
                cx.warnings.push(format!("abandon: unknown settlement {settlement}"));
                "skipped".into()
            }
        }
    }
}
