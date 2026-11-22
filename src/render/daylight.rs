//! Time of day: where the sun is and how the light and sky change with it,
//! through dawn and dusk twilight to a moonlit night (the clock hurries over
//! the night; see world/clock.rs).
//!
//! The style's look (mod.rs `look`) is the mid-afternoon light the game was
//! tuned in. Keys by sun elevation say how far to move away from it: toward
//! twilight blues and pinks with the sun down, oranges at sunset, gold in the
//! hour after, and a whiter, cooler light at midday.

use super::{Look, Sun};
use glam::Vec3;

/// The sun's daily circle in this world's sky, and how its light is allowed
/// to fall. A world sets its latitude and the light when it loads
/// (`set_path`, from world.json `sun`), and the season moves the declination
/// as the year turns (`set_declination`, world/season.rs `Phase`).
///
/// The default latitude, 42, puts the summer sun 62 degrees up at noon, the
/// height the game was tuned under; winter's sun stands 28 degrees lower.
/// However high the sun is, the clock (world/clock.rs) runs on the
/// `REFERENCE` day and maps it through `solar`, so a short winter day and a
/// long summer one take the same real minutes of sunrise, day, sunset and
/// night: only the sun's height and the hour on the clock change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SunPath {
    /// Degrees north. The celestial pole stands this high over the north.
    pub latitude: f32,
    /// The sun's declination, degrees: 0 at the equinox, +14 in summer and
    /// -14 in winter here (the real sky swings 23; the game keeps it gentler).
    pub declination: f32,
    /// Lighting and shadows never use a sun lower than this (degrees). The
    /// sky, the sun's disk and the colour of the light follow the true sun;
    /// only the direction the land is lit from is lifted, so a low winter sun
    /// lights the snow and casts shadows of a sensible length instead of
    /// raking the land and smearing every tree a hundred metres long.
    pub lift_deg: f32,
    /// How much wider than usual the shadow filter spreads (1 = as built).
    pub shadow_soft: f32,
    /// How far the hour's light is turned cold, 0 to 1 (`chill`): a pale sun,
    /// blue shade, a blue-pastel sky and no golden grade, for a world where a
    /// low sun means winter rather than a summer evening.
    pub cold: f32,
}

/// The longest and shortest half-days `solar` allows, in hours, so the
/// clock never stops at a pole (a world far enough north would otherwise
/// have no day at all in winter).
const HALF_DAY_RANGE: (f32, f32) = (1.0, 11.0);

impl SunPath {
    /// A summer sun at the default latitude.
    pub const DEFAULT: SunPath = SunPath {
        latitude: 42.0,
        declination: 14.0,
        lift_deg: 3.44,
        shadow_soft: 1.0,
        cold: 0.0,
    };
    /// The day the clock's rates were fitted to (world/clock.rs): the equinox
    /// at latitude 28, up at 6 due east, 62 degrees at noon, down at 18.
    pub const REFERENCE: SunPath = SunPath {
        latitude: 28.0,
        declination: 0.0,
        lift_deg: 3.44,
        shadow_soft: 1.0,
        cold: 0.0,
    };

    /// Direction toward the sun at `hour` on this path.
    pub fn dir(&self, hour: f32) -> Vec3 {
        let (sl, cl) = self.latitude.to_radians().sin_cos();
        let (sd, cd) = self.declination.to_radians().sin_cos();
        let (sh, ch) = ((hour - 12.0) / 12.0 * std::f32::consts::PI).sin_cos();
        // The highest point of the equator (south, up), the pole, and east.
        let south = Vec3::new(0.0, cl, sl);
        let pole = Vec3::new(0.0, sl, -cl);
        (cd * (ch * south - sh * Vec3::X) + sd * pole).normalize()
    }

    /// The sun's height at noon, degrees.
    #[cfg(test)]
    pub fn noon_deg(&self) -> f32 {
        90.0 - self.latitude + self.declination
    }

    /// The hour in the evening (or, with `evening` false, the morning) the
    /// sun stands `elev` degrees high, if it ever does.
    pub fn hour_at(&self, elev: f32, evening: bool) -> Option<f32> {
        let (sl, cl) = self.latitude.to_radians().sin_cos();
        let (sd, cd) = self.declination.to_radians().sin_cos();
        let c = (elev.to_radians().sin() - sl * sd) / (cl * cd);
        if !(-1.0..=1.0).contains(&c) {
            return None;
        }
        let h = c.acos() / std::f32::consts::PI * 12.0;
        Some(if evening { 12.0 + h } else { 12.0 - h })
    }

    /// Hours from noon to sunset (the sun's centre on the horizon), kept
    /// inside `HALF_DAY_RANGE`.
    pub fn half_day(&self) -> f32 {
        let (tl, td) = (self.latitude.to_radians().tan(), self.declination.to_radians().tan());
        let h = (-tl * td).clamp(-1.0, 1.0).acos() / std::f32::consts::PI * 12.0;
        h.clamp(HALF_DAY_RANGE.0, HALF_DAY_RANGE.1)
    }

    /// The hour on this path that plays the part `hour` plays on the
    /// reference day: sunrise maps to sunrise, noon to noon, sunset to
    /// sunset, and the night is stretched or squeezed between. The clock,
    /// the N key's presets and the launch screen's hours all go through it,
    /// so SUNSET is a sunset everywhere.
    pub fn solar(&self, hour: f32) -> f32 {
        let hd = self.half_day();
        let x = hour.rem_euclid(24.0) - 12.0;
        let d = x.abs();
        let m = if d <= 6.0 { d * hd / 6.0 } else { hd + (d - 6.0) * (12.0 - hd) / 6.0 };
        (12.0 + m * x.signum()).rem_euclid(24.0)
    }

    /// The inverse of `solar`: the reference hour that plays the part of
    /// `hour` here, and how many of this path's hours one reference hour is
    /// at that moment.
    pub fn unsolar(&self, hour: f32) -> (f32, f32) {
        let hd = self.half_day();
        let x = hour.rem_euclid(24.0) - 12.0;
        let d = x.abs();
        let (r, slope) = if d <= hd {
            (d * 6.0 / hd, hd / 6.0)
        } else {
            (6.0 + (d - hd) * 6.0 / (12.0 - hd), (12.0 - hd) / 6.0)
        };
        ((12.0 + r * x.signum()).rem_euclid(24.0), slope)
    }
}

static PATH: std::sync::RwLock<SunPath> = std::sync::RwLock::new(SunPath::DEFAULT);

/// Put this world's sun in the sky. Call it before the clock or the renderer
/// reads an hour; a world without a `sun` block passes `SunPath::DEFAULT`.
pub fn set_path(p: SunPath) {
    *PATH.write().unwrap() = p;
}

/// The sun path in force.
pub fn path() -> SunPath {
    *PATH.read().unwrap()
}

/// Move the sun for the time of year (world/season.rs `Phase::declination`).
pub fn set_declination(d: f32) {
    PATH.write().unwrap().declination = d;
}

/// Direction toward the sun at `hour` (may point below the horizon), on the
/// path in force. It crosses the south (+z) at noon.
pub fn sun_dir(hour: f32) -> Vec3 {
    path().dir(hour)
}

/// The axis the sun turns about through the day: the line to the celestial
/// pole, square to every `sun_dir` on the default path and at a fixed angle
/// to it on any other. Shadow maps use it as their up direction; see render
/// `cascade_matrix`.
pub fn path_axis() -> Vec3 {
    let (sl, cl) = path().latitude.to_radians().sin_cos();
    Vec3::new(0.0, -sl, cl)
}

/// How fast the moon moves along the sun's path, in the sun's hours per real
/// minute: a little faster than the sun by day, and steady, however much the
/// clock hurries through the night.
const MOON_RATE: f32 = 1.0;

/// Direction toward the moon while it is the key light. On the default path
/// it crosses the south 62 degrees up halfway through the night, so it is
/// high in the east-south-east at dusk (about 38 degrees) and as high in the
/// west-south-west when the sun comes up and it fades. It runs the sun's
/// circle with the declination turned over, as a full moon does, so under a
/// low winter sun the moon rides high.
pub fn moon_dir(hour: f32) -> Vec3 {
    let (now, total) = crate::world::clock::night_minutes(hour);
    let p = path();
    SunPath { declination: -p.declination, ..p }.dir(12.0 + (now - total * 0.5) * MOON_RATE)
}

struct Key {
    elev: f32,
    sun: [f32; 3],
    gain: f32,
    zenith: [f32; 3],
    horizon: [f32; 3],
    ambient: [f32; 3],
    ground: f32,
    fog: f32,
    /// How far these values replace the style's own (0 = not at all).
    w: f32,
}

const KEYS: [Key; 10] = [
    // Night: about a quarter darker than the deepest twilight, a pale moonlit
    // blue rather than violet, and foggier.
    Key {
        elev: -18.0,
        sun: [1.0, 0.3, 0.15],
        gain: 0.0,
        zenith: [0.006, 0.011, 0.034],
        horizon: [0.075, 0.10, 0.15],
        ambient: [0.15, 0.18, 0.27],
        ground: 0.28,
        fog: 1.6,
        w: 1.0,
    },
    // Deep twilight: an indigo sky with a violet band low down, lit by the moon
    // (see `apply`). Darker than dusk, never black.
    Key {
        elev: -12.0,
        sun: [1.0, 0.3, 0.15],
        gain: 0.0,
        zenith: [0.007, 0.009, 0.038],
        horizon: [0.11, 0.08, 0.19],
        ambient: [0.20, 0.21, 0.38],
        ground: 0.35,
        fog: 1.3,
        w: 1.0,
    },
    // Blue hour: purple and rose, the warm band fading from the west.
    Key {
        elev: -6.0,
        sun: [1.0, 0.3, 0.15],
        gain: 0.0,
        zenith: [0.02, 0.026, 0.10],
        horizon: [0.36, 0.21, 0.36],
        ambient: [0.30, 0.28, 0.44],
        ground: 0.45,
        fog: 1.3,
        w: 1.0,
    },
    Key {
        elev: -2.0,
        sun: [1.0, 0.32, 0.14],
        gain: 0.0,
        zenith: [0.08, 0.10, 0.30],
        horizon: [0.80, 0.42, 0.40],
        ambient: [0.40, 0.34, 0.46],
        ground: 0.6,
        fog: 1.25,
        w: 1.0,
    },
    // Sunset: the sun on the horizon, deep orange.
    Key {
        elev: 0.5,
        sun: [1.0, 0.42, 0.16],
        gain: 0.8,
        zenith: [0.12, 0.20, 0.46],
        horizon: [1.0, 0.56, 0.30],
        ambient: [0.44, 0.37, 0.42],
        ground: 0.75,
        fog: 1.2,
        w: 1.0,
    },
    // Golden hour: a saturated gold sun, warm haze, warm bounce in the shade.
    Key {
        elev: 5.0,
        sun: [1.0, 0.58, 0.26],
        gain: 0.98,
        zenith: [0.14, 0.28, 0.60],
        horizon: [1.0, 0.70, 0.40],
        ambient: [0.48, 0.42, 0.42],
        ground: 0.95,
        fog: 1.15,
        w: 1.0,
    },
    Key {
        elev: 11.0,
        sun: [1.0, 0.68, 0.36],
        gain: 1.06,
        zenith: [0.15, 0.34, 0.70],
        horizon: [1.0, 0.80, 0.52],
        ambient: [0.48, 0.50, 0.50],
        ground: 1.0,
        fog: 1.1,
        w: 0.92,
    },
    // Fades into the afternoon look the styles were tuned in.
    Key {
        elev: 20.0,
        sun: [1.0, 0.76, 0.48],
        gain: 1.0,
        zenith: [0.13, 0.38, 0.78],
        horizon: [0.86, 0.82, 0.72],
        ambient: [0.47, 0.58, 0.60],
        ground: 1.0,
        fog: 1.0,
        w: 0.45,
    },
    Key {
        elev: 32.0,
        sun: [1.0, 0.78, 0.52],
        gain: 1.0,
        zenith: [0.13, 0.38, 0.78],
        horizon: [0.82, 0.80, 0.74],
        ambient: [0.47, 0.58, 0.60],
        ground: 1.0,
        fog: 1.0,
        w: 0.0,
    },
    // Midday: whiter sun, deeper zenith, paler horizon, more fill.
    Key {
        elev: 62.0,
        sun: [1.0, 0.96, 0.9],
        gain: 1.06,
        zenith: [0.09, 0.32, 0.82],
        horizon: [0.72, 0.82, 0.92],
        ambient: [0.52, 0.64, 0.70],
        ground: 1.0,
        fog: 0.9,
        w: 0.6,
    },
];

/// Colour of the light the air scatters toward the viewer around the sun
/// (haze, mist and the afterglow in the sky), by sun elevation.
const GLOW: [(f32, [f32; 3]); 8] = [
    (-18.0, [0.12, 0.15, 0.22]),
    (-12.0, [0.20, 0.12, 0.30]),
    (-7.0, [0.42, 0.22, 0.42]),
    (-3.0, [0.80, 0.36, 0.34]),
    (1.0, [1.0, 0.50, 0.24]),
    (8.0, [0.95, 0.66, 0.34]),
    (22.0, [0.70, 0.56, 0.38]),
    (45.0, [0.40, 0.38, 0.34]),
];

/// Moonlight: a cool, whitish key light once the sun has gone (times the
/// sun's base intensity). In the night proper it dims by `MOON_NIGHT`, since
/// the moon there stands higher than it does at twilight and lights more.
const MOON: [f32; 3] = [0.66, 0.72, 0.98];
const MOON_GAIN: f32 = 0.22;
const MOON_NIGHT: f32 = 0.5;

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// What the time of day adds beyond the style's sky and sun.
#[derive(Clone, Copy, Debug)]
pub struct Air {
    /// Light the air scatters toward the viewer near the sun, and how strongly
    /// haze picks it up (0..1).
    pub glow: Vec3,
    pub inscatter: f32,
    /// Toward the real sun, even below the horizon (the key light may be the moon).
    pub sun: Vec3,
    /// 0 by day, 1 in deep twilight: stars and the moon's disk.
    pub night: f32,
    /// Multiplier on biome mist, and a thin mist everywhere at dawn and dusk.
    pub mist_scale: f32,
    pub mist_floor: f32,
    /// The grade's shift: up to 1 at golden hour (warmer), down to -1 by
    /// moonlight (cooler).
    pub warmth: f32,
}

/// Palette study (VOXEL_GRADE=pal1 or pal2, saturated style only): an art-directed palette
/// over the time of day's own. 1 exaggerates golden hour (orange sun, rose
/// horizon, violet zenith); 2 is a smoke-haze day, as under a wildfire.
pub fn palette_study(lk: &mut Look, sun: &mut Sun, air: &mut Air) {
    static PAL: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    let v = *PAL.get_or_init(|| super::shaders::grade_variant("pal"));
    let lum = |c: [f32; 3]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
    // `to` at the brightness of `from`.
    let at = |from: [f32; 3], to: [f32; 3]| {
        let k = lum(from) / lum(to).max(1e-4);
        to.map(|c| c * k)
    };
    match v {
        3 => {
            // The default: option 1 pushed a fifth further, faded in by the
            // golden-hour warmth so midday is untouched.
            let a = (air.warmth.clamp(0.0, 1.0) * 1.2).min(1.2);
            if a > 0.0 {
                sun.color *= Vec3::ONE.lerp(Vec3::new(1.12, 0.86, 0.58), a);
                lk.horizon = mix3(lk.horizon, at(lk.horizon, [1.0, 0.64, 0.56]), 0.6 * a);
                lk.zenith = mix3(lk.zenith, at(lk.zenith, [0.30, 0.26, 0.62]), 0.6 * a);
                lk.ambient = mix3(lk.ambient, at(lk.ambient, [0.62, 0.52, 0.66]), 0.45 * a);
                air.glow = air.glow.lerp(air.glow.max(Vec3::new(0.9, 0.42, 0.3)) * 1.3, a.min(1.0));
                air.inscatter = air.inscatter + (air.inscatter.max(0.6) - air.inscatter) * a.min(1.0);
            }
        }
        1 => {
            sun.color *= Vec3::new(1.12, 0.86, 0.58);
            lk.horizon = mix3(lk.horizon, at(lk.horizon, [1.0, 0.64, 0.56]), 0.6);
            lk.zenith = mix3(lk.zenith, at(lk.zenith, [0.30, 0.26, 0.62]), 0.6);
            lk.ambient = mix3(lk.ambient, at(lk.ambient, [0.62, 0.52, 0.66]), 0.45);
            air.glow = air.glow.max(Vec3::new(0.9, 0.42, 0.3)) * 1.3;
            air.inscatter = air.inscatter.max(0.6);
            air.warmth = air.warmth.max(0.8);
        }
        2 => {
            sun.color *= Vec3::new(1.0, 0.6, 0.32) * 0.85;
            lk.horizon = at(lk.horizon, [0.92, 0.62, 0.40]);
            lk.zenith = at(lk.zenith, [0.62, 0.52, 0.46]).map(|c| c * 0.9);
            lk.ambient = mix3(lk.ambient, at(lk.ambient, [0.72, 0.56, 0.44]), 0.6);
            lk.fog_density *= 2.4;
            air.glow = Vec3::new(1.0, 0.5, 0.22);
            air.inscatter = air.inscatter.max(0.7);
            air.warmth = air.warmth.max(0.6);
        }
        _ => {}
    }
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// What the weather does to the hour's light (world/weather.rs). Run after
/// `apply`, over whatever the time of day decided, so a wet dusk is still a
/// dusk. Fair weather is every multiplier 1 and leaves all three alone.
///
/// Overcast is the one that does most of it: the key light loses its edge and
/// goes grey, the sky fill comes up to take its place (a cloud deck is a
/// bright thing, however dark the ground under it reads), the horizon pales
/// toward the zenith and the sun's glow in the air goes out. The haze and the
/// mist are multipliers the rest of the renderer already had.
pub fn weather(lk: &mut Look, sun: &mut Sun, air: &mut Air, sky: &crate::world::weather::Sky) {
    let o = sky.overcast.clamp(0.0, 1.0);
    if o > 0.0 {
        // The key light: dimmer, and its colour drawn toward the flat grey-
        // white of light that has been through a cloud.
        let grey = Vec3::splat(sun.color.length() / 3f32.sqrt());
        sun.color = sun.color.lerp(grey, o * 0.75);
        sun.intensity *= 1.0 - 0.55 * o;
        // The sky fills in for it, a touch cooler than the open sky's fill.
        let fill = mix3(lk.ambient, [0.62, 0.66, 0.70], 0.5);
        lk.ambient = mix3(lk.ambient, fill.map(|c| c * 1.25), o * 0.8);
        // A closed sky is one tone: the horizon comes up to meet the zenith,
        // both toward a pale grey of the sky's own brightness.
        let flat = mix3(lk.zenith, lk.horizon, 0.6);
        let lum = flat[0] * 0.2126 + flat[1] * 0.7152 + flat[2] * 0.0722;
        let pale = mix3(flat, [lum * 1.35, lum * 1.38, lum * 1.42], 0.65);
        lk.zenith = mix3(lk.zenith, pale, o * 0.8);
        lk.horizon = mix3(lk.horizon, pale.map(|c| c * 1.08), o * 0.85);
        // No sun to scatter round, so the warm pool near it fades.
        air.glow = air.glow.lerp(Vec3::splat(air.glow.length() / 3f32.sqrt()), o * 0.8);
        air.inscatter *= 1.0 - 0.5 * o;
    }
    lk.fog_density *= sky.haze.max(0.0);
    air.mist_scale *= 1.0 + 2.0 * sky.mist;
    air.mist_floor += sky.mist;
    air.warmth += sky.warmth;
}

/// Turn the hour's light cold by `cold` (0 to 1), keeping its brightness: the
/// sun a pale white-peach, the fill and the zenith blue, the horizon a
/// pastel between. Returns the sun's colour.
fn chill(lk: &mut Look, sun: Vec3, cold: f32) -> Vec3 {
    if cold <= 0.0 {
        return sun;
    }
    let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let to = |c: [f32; 3], hue: [f32; 3]| {
        let v = Vec3::from(c);
        let h = Vec3::from(hue);
        let t = h * (luma(v) / luma(h));
        v.lerp(t, cold).to_array()
    };
    lk.zenith = to(lk.zenith, [0.55, 0.72, 1.0]);
    lk.horizon = to(lk.horizon, [0.92, 0.9, 1.0]);
    lk.ambient = to(lk.ambient, [0.62, 0.76, 1.0]);
    let h = Vec3::new(1.0, 0.93, 0.86);
    sun.lerp(h * (luma(sun) / luma(h)), cold)
}

/// The look and key light for `hour`, from the style's look and its default
/// sun, plus the air. With the sun more than 3 degrees down the key light is
/// the moon; its colour fades through zero at the switch, so the shadows'
/// jump to the moon's direction is never seen.
pub fn apply(mut lk: Look, base: &Sun, hour: f32) -> (Look, Sun, Air) {
    let dir = sun_dir(hour);
    let elev = dir.y.asin().to_degrees();
    let i = KEYS.iter().position(|k| k.elev > elev).unwrap_or(KEYS.len());
    let (a, b, t) = if i == 0 {
        (&KEYS[0], &KEYS[0], 0.0)
    } else if i == KEYS.len() {
        (&KEYS[i - 1], &KEYS[i - 1], 0.0)
    } else {
        let (a, b) = (&KEYS[i - 1], &KEYS[i]);
        (a, b, (elev - a.elev) / (b.elev - a.elev))
    };
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    let w = lerp(a.w, b.w);
    let style_sun = base.color * lk.sun_tint;
    let key_sun = Vec3::from(mix3(a.sun, b.sun, t));
    // Below the horizon the sun's disk and glow go out of the sky too.
    let up = ((elev + 3.0) / 3.5).clamp(0.0, 1.0);
    let sun_color = style_sun.lerp(key_sun, w) * (up * up * (3.0 - 2.0 * up));
    let gain = 1.0 + (lerp(a.gain, b.gain) - 1.0) * w;
    lk.zenith = mix3(lk.zenith, mix3(a.zenith, b.zenith, t), w);
    lk.horizon = mix3(lk.horizon, mix3(a.horizon, b.horizon, t), w);
    lk.ambient = mix3(lk.ambient, mix3(a.ambient, b.ambient, t), w);
    let g = 1.0 + (lerp(a.ground, b.ground) - 1.0) * w;
    lk.ground = lk.ground.map(|c| c * g);
    lk.fog_density *= 1.0 + (lerp(a.fog, b.fog) - 1.0) * w;
    lk.sun_tint = Vec3::ONE;
    let cold = path().cold.clamp(0.0, 1.0);
    let sun_color = chill(&mut lk, sun_color, cold);
    // Lighting and shadows never use a sun lower than the path's lift (about
    // 3.5 degrees by default): below that the shadow maps smear across the
    // land, and by then the gain is low. Lifted at a fixed azimuth.
    let lift = path().lift_deg.to_radians();
    let lit = if dir.y < lift.sin() {
        let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::X);
        flat * lift.cos() + Vec3::Y * lift.sin()
    } else {
        dir
    };
    let gi = GLOW.iter().position(|k| k.0 > elev).unwrap_or(GLOW.len());
    let glow = if gi == 0 {
        Vec3::from(GLOW[0].1)
    } else if gi == GLOW.len() {
        Vec3::from(GLOW[gi - 1].1)
    } else {
        let (a, b) = (GLOW[gi - 1], GLOW[gi]);
        Vec3::from(a.1).lerp(Vec3::from(b.1), (elev - a.0) / (b.0 - a.0))
    };
    let low = smooth(-6.0, 6.0, elev) * (1.0 - smooth(6.0, 30.0, elev));
    let deep = smooth(-11.0, -18.0, elev);
    let mut air = Air {
        glow,
        inscatter: 0.3 + 0.7 * smooth(32.0, 6.0, elev),
        sun: dir,
        night: smooth(-3.5, -10.0, elev),
        // Mornings are mistier than evenings, and the night mistier still.
        mist_scale: 1.0 + (if hour < 12.0 { 0.8 } else { 0.45 }) * smooth(14.0, 0.0, elev) + 0.5 * deep,
        mist_floor: 0.14 * smooth(5.0, -3.0, elev) + 0.04 * low + 0.08 * deep,
        // Below -1 the grade turns from violet twilight to the night's pale
        // blue (post.wgsl).
        warmth: smooth(-3.0, 3.0, elev) * (1.0 - smooth(14.0, 26.0, elev)) - smooth(-4.0, -10.0, elev) - 0.5 * deep,
    };
    if cold > 0.0 {
        // No golden grade; the warm pool round the sun turns rose-white.
        air.warmth = air.warmth * (1.0 - cold) - 0.3 * cold;
        let l = air.glow.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        air.glow = air.glow.lerp(Vec3::new(1.0, 0.86, 0.9) * l, cold);
    }
    if elev < -3.0 {
        // The moon fades in as the sun's light fades out, and out again as the
        // sun comes back.
        let m = smooth(-3.0, -7.0, elev);
        let dir = moon_dir(hour);
        let moon = Sun {
            dir,
            color: Vec3::from(MOON) * m,
            intensity: base.intensity * MOON_GAIN * (1.0 - MOON_NIGHT * deep),
        };
        return (lk, moon, air);
    }
    (
        lk,
        Sun {
            dir: lit,
            color: sun_color,
            intensity: base.intensity * gain,
        },
        air,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::clock::{dawn, dusk, DAWN, DUSK};

    #[test]
    fn the_sun_rises_in_the_east_and_peaks_in_the_south() {
        let r = SunPath::REFERENCE;
        assert!(r.dir(6.0).x > 0.99);
        assert!(r.dir(18.0).x < -0.99);
        let noon = r.dir(12.0);
        assert!(noon.z > 0.4 && (noon.y.asin().to_degrees() - 62.0).abs() < 0.5);
        assert!(r.dir(DAWN).y < -0.05 && r.dir(DUSK).y < -0.05);
        assert!(sun_dir(dawn()).y < -0.05 && sun_dir(dusk()).y < -0.05);
    }

    #[test]
    fn the_seasons_move_the_sun_and_the_clock_keeps_the_day() {
        let summer = SunPath::DEFAULT;
        assert!((summer.noon_deg() - 62.0).abs() < 1e-3);
        assert!((summer.dir(12.0).y.asin().to_degrees() - 62.0).abs() < 0.5);
        let winter = SunPath { declination: -14.0, ..summer };
        assert!((winter.noon_deg() - 34.0).abs() < 1e-3);
        assert!(winter.half_day() < 5.5 && summer.half_day() > 6.5);
        // Sunrise on each path is where the reference sunrise maps to.
        for p in [
            summer,
            winter,
            SunPath {
                latitude: 64.0,
                declination: -14.0,
                ..summer
            },
        ] {
            let rise = p.solar(6.0);
            assert!(p.dir(rise).y.abs() < 0.01, "{p:?}: {}", p.dir(rise).y);
            for h in [0.5, 5.0, 9.0, 12.0, 15.5, 19.0, 23.9] {
                let (r, _) = p.unsolar(p.solar(h));
                assert!((r - h).abs() < 1e-3, "{h} -> {r}");
            }
        }
    }

    #[test]
    fn the_moon_takes_over_after_sunset() {
        let base = Sun::default();
        let lk = || super::super::look(super::super::Style::Saturated);
        let (_, sun, air) = apply(lk(), &base, 17.0);
        assert!(sun.dir.x < -0.5 && air.night == 0.0);
        let (_, moon, air) = apply(lk(), &base, dusk());
        assert!(moon.dir.x > 0.3 && moon.dir.y > 0.3, "the moon is up in the east: {:?}", moon.dir);
        assert!(moon.color.length() > 0.5 && air.night > 0.9);
        // It crosses the south around midnight, and the night is a pale blue.
        let (night, moon, _) = apply(lk(), &base, 23.5);
        assert!(moon.dir.z > 0.4 && moon.dir.y > 0.5);
        assert!(night.ambient[2] > night.ambient[0] * 1.5 && night.ambient[1] > night.ambient[0]);
        // And is still up in the west when the sun's light returns.
        let m = moon_dir(dawn() + 0.5);
        assert!(m.x < -0.3 && m.y > 0.3, "{m:?}");
        // The key light is dark where the sun hands over to the moon.
        let switch = (0..20000).map(|i| 17.9 + i as f32 * 0.0001).find(|h| sun_dir(*h).y.asin().to_degrees() < -3.0).unwrap();
        let (_, a, _) = apply(lk(), &base, switch - 0.0002);
        let (_, b, _) = apply(lk(), &base, switch + 0.0002);
        assert!(a.color.length() < 0.01 && b.color.length() < 0.01);
    }

    /// Fair weather leaves the hour's own light exactly as it was, and a
    /// closed sky dims the key light, lifts the fill, flattens the sky and
    /// thickens the haze.
    #[test]
    fn the_weather_goes_over_the_hour() {
        use crate::world::weather::Kind;
        let base = Sun::default();
        let at = |h: f32, k: Kind| {
            let (mut lk, mut sun, mut air) = apply(super::super::look(super::super::Style::Saturated), &base, h);
            weather(&mut lk, &mut sun, &mut air, &k.sky());
            (lk, sun, air)
        };
        for h in [8.0, 12.0, 15.0, 17.5, 23.0] {
            let (fair, fsun, fair_air) = at(h, Kind::Sun);
            let (was, wsun, was_air) = {
                let (lk, sun, air) = apply(super::super::look(super::super::Style::Saturated), &base, h);
                (lk, sun, air)
            };
            assert_eq!(fair.zenith, was.zenith, "fair weather at {h} moved the sky");
            assert_eq!(fair.horizon, was.horizon);
            assert_eq!(fair.ambient, was.ambient);
            assert_eq!(fair.fog_density, was.fog_density);
            assert_eq!(fsun.color, wsun.color);
            assert_eq!(fsun.intensity, wsun.intensity);
            assert_eq!(fair_air.mist_floor, was_air.mist_floor);
            assert_eq!(fair_air.warmth, was_air.warmth);

            // Rain, at the same hour: a weaker, greyer key light, more fill,
            // a flatter sky, thicker air, cooler grade.
            let (wet, rsun, wet_air) = at(h, Kind::Rain);
            assert!(rsun.intensity < wsun.intensity * 0.6, "at {h}: {} vs {}", rsun.intensity, wsun.intensity);
            let spread = |c: Vec3| (c.max_element() - c.min_element()) / c.max_element().max(1e-4);
            assert!(spread(rsun.color) < spread(wsun.color) + 1e-6, "at {h} the wet key light is greyer");
            let lum = |c: [f32; 3]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
            assert!(lum(wet.ambient) > lum(was.ambient), "at {h} the sky fills in");
            let gap = |l: &Look| (lum(l.horizon) - lum(l.zenith)).abs();
            assert!(gap(&wet) < gap(&was), "at {h} the sky flattens");
            assert!(wet.fog_density > was.fog_density * 1.5);
            assert!(wet_air.mist_floor > was_air.mist_floor);
            assert!(wet_air.warmth < was_air.warmth);
            assert!(wet_air.inscatter < was_air.inscatter);
        }
        // Fog thickens the air far more than rain, and the heat warms it.
        let (foggy, _, fog_air) = at(12.0, Kind::Fog);
        let (wet, _, _) = at(12.0, Kind::Rain);
        assert!(foggy.fog_density > wet.fog_density && fog_air.mist_floor > 0.5);
        let (_, _, hot) = at(12.0, Kind::Heat);
        let (_, _, plain) = at(12.0, Kind::Sun);
        assert!(hot.warmth > plain.warmth);
    }

    #[test]
    fn the_sun_turns_about_the_path_axis() {
        // At a fixed angle to it all day: square on the equinox, off by the
        // declination otherwise.
        let d = path().declination.to_radians().sin();
        for h in [5.5, 9.0, 12.0, 16.0, 18.5] {
            assert!((sun_dir(h).dot(path_axis()) + d).abs() < 1e-5);
        }
    }
}
