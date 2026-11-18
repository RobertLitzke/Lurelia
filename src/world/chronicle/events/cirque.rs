use crate::world::chronicle::geom::each_near;
use crate::world::chronicle::{EventCtx, Process};
use crate::world::fields::{Fields, Poi, Pos};
use crate::world::noise::{perlin2, smoothstep};
use glam::Vec2;
use serde::Deserialize;

/// A cirque: the armchair bowl a glacier gnaws into a mountainside, with a
/// near-sheer headwall round three sides and the fourth open downhill
/// (`opens`, a bearing in degrees: 0 north, 90 east), where it spills into
/// the valley below. Its floor is a hollow at `floor` (absolute, m), the
/// headwall curves up toward `rim`. It only lowers the ground, and then,
/// with `ice`, lays a glacier in the bowl: a domed tongue `ice_m` thick that
/// runs out of the mouth, cut across by crevasses where it bends over the
/// lip.
#[derive(Deserialize, Debug)]
pub struct Cirque {
    at: Pos,
    radius: f32,
    floor: f32,
    rim: f32,
    opens: f32,
    /// Material of the glacier's surface; no glacier if unset.
    #[serde(default)]
    ice: Option<String>,
    #[serde(default = "default_ice_m")]
    ice_m: f32,
    #[serde(default)]
    poi: Option<String>,
}

fn default_ice_m() -> f32 {
    6.0
}

impl Process for Cirque {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let c = cx.pos(&self.at);
        let ice = self.ice.as_ref().map(|m| cx.mat(m));
        let (seed, cause) = (cx.seed, cx.cause);
        let r = self.radius.max(4.0);
        // Bearing 0 is north (-z), 90 east (+x).
        let b = self.opens.to_radians();
        let out = Vec2::new(b.sin(), -b.cos());
        let (floor, rim, ice_m) = (self.floor, self.rim, self.ice_m);
        let mut cut = 0;
        let mut iced = 0;
        each_near(&mut cx.f, c, r * 2.2, |f: &mut Fields, i, _d, _| {
            let p = f.centre(i);
            let v = p - c;
            let ahead = v.dot(out);
            let side = v.perp_dot(out);
            // The bowl is round behind the centre and drawn out ahead of it,
            // so it opens into a mouth rather than closing in a crater.
            let stretch = if ahead > 0.0 { 0.45 } else { 1.0 };
            let wob = 1.0 + 0.12 * perlin2(p.x * 0.03, p.y * 0.03, seed);
            let q = (side * side + (ahead * stretch) * (ahead * stretch)).sqrt() / (r * wob);
            if q >= 1.0 {
                return;
            }
            // Flat in the middle, then the headwall: a cube curve that ends
            // near vertical at the rim.
            let target = floor + (rim - floor) * q.powi(3);
            if target < f.h[i] - 0.05 {
                f.h[i] = target;
                f.cause[i] = cause;
                f.surface[i] = 0;
                f.cliff[i] = if q > 0.62 { 1.0 } else { 0.0 };
                cut += 1;
            }
            // The glacier: thickest in the hollow, thinning to the headwall's
            // foot and out along the mouth.
            if let Some(m) = ice {
                let reach = 1.0 - smoothstep(0.55, 0.8, q);
                if reach > 0.0 && f.h[i] < floor + (rim - floor) * 0.25 {
                    let mut t = ice_m * reach;
                    // Crevasses: grooves across the flow, where the ice
                    // bends over the lip toward the mouth.
                    let across = ahead * 0.22 + perlin2(p.x * 0.05, p.y * 0.05, seed + 1) * 2.0;
                    let crack = smoothstep(0.93, 0.99, across.sin().abs()) * smoothstep(-0.1, 0.4, ahead / r);
                    t -= crack * ice_m.min(4.0) * 0.3;
                    f.h[i] += t.max(0.3);
                    f.surface[i] = m;
                    f.cliff[i] = 0.0;
                    iced += 1;
                }
            }
        });
        if let Some(name) = &self.poi {
            cx.pois.push(Poi { name: name.clone(), pos: c, cause });
        }
        format!("cirque r {r:.0} m, floor {floor:.0} m, {cut} m2 cut, glacier {iced} m2")
    }
}
