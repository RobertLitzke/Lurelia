use crate::world::chronicle::{EventCtx, Process};
use crate::world::city::{self, CityFile, Kit};
use serde::Deserialize;

/// A city from map data (a `city.json` in the world folder): streets and
/// pavements painted, parks and woods as regions `city_park`/`city_wood`/
/// `city_commercial`, buildings levelled and drawn from their footprints,
/// bridge decks and piers.
#[derive(Deserialize, Debug)]
pub struct City {
    file: String,
    /// A city kit in `library/cities/` (material roles, swaps, ivy, more
    /// trees). Left out, the city is made of what the importer meant it to
    /// be (`modern`).
    #[serde(default)]
    kit: Option<String>,
}

impl Process for City {
    fn apply(&self, cx: &mut EventCtx) -> String {
        let City { file, kit } = self;
        let (seed, cause) = (cx.seed, cx.cause);
        let text = match std::fs::read_to_string(cx.dir.join(file)) {
            Ok(t) => t,
            Err(e) => {
                cx.warnings.push(format!("city {file}: {e}"));
                return "city missing".into();
            }
        };
        // An importer may also write metadata the game doesn't read
        // (source ids, names and kinds, the centre and datum); any other
        // unread key is reported once, with how often it occurs.
        const METADATA: [&str; 9] = ["id", "name", "kind", "centre", "water_level_navd88", "places", "sea", "size", "scale"];
        let mut unread = std::collections::BTreeMap::<String, usize>::new();
        let parsed = serde_json::from_str(&text).and_then(|v| {
            crate::library::parse_known(v, |k| {
                if METADATA.contains(&k.rsplit('.').next().unwrap_or(&k)) {
                    return;
                }
                let general: String = k.split('.').map(|s| if s.bytes().all(|b| b.is_ascii_digit()) { "*" } else { s }).collect::<Vec<_>>().join(".");
                *unread.entry(general).or_default() += 1;
            })
        });
        for (k, n) in unread {
            cx.warnings.push(format!("city {file}: key {k} not read ({n}x)"));
        }
        let cf: CityFile = match parsed {
            Ok(c) => c,
            Err(e) => {
                cx.warnings.push(format!("city {file}: {e}"));
                return "city unreadable".into();
            }
        };
        let lib = cx.lib;
        let mut missing = std::collections::BTreeSet::new();
        // The kit the world asked for, or the plain one. A name nothing
        // answers to is reported rather than quietly ignored: a mistyped kit
        // would otherwise leave the city looking untouched.
        let fallback = Kit::default();
        let kit = match kit.as_deref() {
            None => cx.lib.city_kit("modern").unwrap_or(&fallback),
            Some(n) => match cx.lib.city_kit(n) {
                Some(k) => k,
                None => {
                    cx.warnings.push(format!("city {file}: no city kit called {n}"));
                    cx.lib.city_kit("modern").unwrap_or(&fallback)
                }
            },
        };
        let (built, msg) = city::build(&mut cx.f, &cf, kit, cause, seed, |name| {
            lib.material(name).unwrap_or_else(|| {
                missing.insert(name.to_string());
                crate::world::material::STONE
            })
        });
        for m in missing {
            cx.warnings.push(format!("city: unknown material {m}"));
        }
        *cx.state.get::<city::City>() = built;
        let signs = place_signs(cx, &cf);
        let regions = name_regions(cx, &cf);
        if signs + regions > 0 {
            return format!("{msg}; {signs} signs, {regions} named places");
        }
        msg
    }
}

/// The import's street blades and name boards: a post each, and a board whose
/// lettering the renderer draws (world/places.rs).
fn place_signs(cx: &mut EventCtx, cf: &CityFile) -> usize {
    let mut n = 0;
    let mut unknown = std::collections::BTreeSet::new();
    for (k, sg) in cf.signs.iter().enumerate() {
        let Some(si) = cx.lib.sign_style(&sg.style) else {
            unknown.insert(sg.style.clone());
            continue;
        };
        let sd = cx.lib.signs[si].clone();
        let p = glam::Vec2::from(sg.at);
        let seed = crate::world::noise::hash32(cx.seed ^ (k as u32).wrapping_mul(0x9e37_79b9));
        let mut front = p;
        // As in events/sign.rs: the board hangs off the foot of its post, not
        // off the ground under the sign's point.
        let mut foot = cx.f.height(p.x, p.y);
        if let Some(post) = &sd.post {
            if let Some(d) = cx.lib.structure(post) {
                if let Some(j) = cx.place(d, p, sg.bearing, &Default::default(), 0.0, seed) {
                    let (x0, z0, x1, z1) = cx.placed[j].footprint(&cx.shapes);
                    let c = glam::Vec2::new((x0 + x1) as f32 * 0.125, (z0 + z1) as f32 * 0.125);
                    let half = glam::Vec2::new((x1 - x0) as f32 * 0.125, (z1 - z0) as f32 * 0.125);
                    let nv = glam::Vec2::new(sg.bearing.to_radians().sin(), -sg.bearing.to_radians().cos());
                    front = c + nv * (nv.x.abs() * half.x + nv.y.abs() * half.y);
                    foot = cx.placed[j].origin[1] as f32 * 0.25;
                }
            }
        }
        let r = sg.bearing.to_radians();
        let board = front + glam::Vec2::new(r.sin(), -r.cos()) * (sd.thickness * 0.5 + sd.offset_m);
        let y = foot + sd.centre_m;
        cx.signs.push(crate::world::places::Sign {
            at: glam::Vec3::new(board.x, y, board.y),
            bearing: sg.bearing,
            style: si,
            text: sg.text.clone(),
        });
        n += 1;
    }
    for s in unknown {
        cx.warnings.push(format!("city: unknown sign style {s}"));
    }
    n
}

/// The import's named places: neighbourhoods, parks, streets.
fn name_regions(cx: &mut EventCtx, cf: &CityFile) -> usize {
    let seed = cx.seed;
    let mut n = 0;
    for (k, r) in cf.regions.iter().enumerate() {
        let kind = if r.kind.is_empty() { "place".to_string() } else { r.kind.clone() };
        let mut area = if r.poly.len() >= 3 {
            crate::world::places::Area::new(r.name.clone(), kind, r.poly.iter().map(|p| glam::Vec2::from(*p)).collect(), 0.0)
        } else if let (Some(at), true) = (r.at, r.radius > 0.0) {
            crate::world::places::Area::disc(r.name.clone(), kind, glam::Vec2::from(at), r.radius, 0.1, seed ^ k as u32)
        } else {
            cx.warnings.push(format!("city region {}: needs an outline, or a centre and a radius", r.name));
            continue;
        };
        let across = (area.hi - area.lo).min_element();
        area.inside_m = r.inside.unwrap_or((across * 0.1).clamp(3.0, 20.0));
        cx.areas.push(area);
        n += 1;
    }
    n
}
