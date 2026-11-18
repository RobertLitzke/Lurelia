//! The game clock: the hour of the day, 0 to 24. How fast it runs depends on
//! where the sun is, so each kind of light gets its own share of a 25-minute
//! day: the night is hurried, sunset and golden hour are drawn out. The
//! renderer turns an hour into light (render/daylight.rs).

use crate::render::daylight::sun_dir;

/// First and last light: the day's activity (animals, photo tasks) runs
/// between them. The sun is about 10.6 degrees down at both.
/// On the reference day; `dawn()` and `dusk()` are this world's.
pub const DAWN: f32 = 5.2;
pub const DUSK: f32 = 18.8;

/// The sun's height at first and last light, degrees.
const FIRST_LIGHT: f32 = -10.6;

/// First light in this world today: the sun 10.6 degrees down, or where a
/// sun that never goes that low plays the part of it.
pub fn dawn() -> f32 {
    let p = crate::render::daylight::path();
    p.hour_at(FIRST_LIGHT, false).unwrap_or_else(|| p.solar(DAWN))
}

/// Last light in this world today.
pub fn dusk() -> f32 {
    let p = crate::render::daylight::path();
    p.hour_at(FIRST_LIGHT, true).unwrap_or_else(|| p.solar(DUSK))
}
/// The hour a new game starts at: mid-afternoon, the light the styles were tuned in.
pub const START: f32 = 15.0;

/// Game hours per real minute while the sun is in each band of elevation:
/// night (below -12 degrees), deep twilight, blue hour, sunrise and sunset,
/// golden hour, warm early and late light, the afternoon look the styles were
/// tuned in, midday (above 50). Fitted numerically so the bands take 15, 7, 3, 6.7, 15.1, 16.3, 17 and 19.9% of a 25-minute day.
const RATES: [f32; 8] = [2.7334, 0.4375, 0.8495, 0.3426, 0.3498, 0.8088, 0.7276, 0.8014];
/// The elevations between the bands, and how many degrees either side the
/// rate blends over, so the sun never lurches.
const EDGES: [(f32, f32); 7] = [(-12.0, 2.0), (-6.0, 1.0), (-2.0, 1.0), (2.0, 1.0), (11.0, 1.5), (32.0, 2.0), (50.0, 2.0)];

/// The hours the N key steps through.
pub const PRESETS: [(f32, &str); 9] = [
    (5.6, "DAWN"),
    (8.0, "MORNING"),
    (12.0, "MIDDAY"),
    (15.0, "AFTERNOON"),
    (17.1, "GOLDEN HOUR"),
    (17.75, "SUNSET"),
    (18.4, "DUSK"),
    (18.75, "TWILIGHT"),
    (23.0, "NIGHT"),
];

/// The next preset after `hour`, wrapping round. The presets are written for
/// the default sky; under another sun path each is moved to the hour that
/// plays the same part (daylight `SunPath::solar`), so SUNSET is a sunset.
pub fn next_preset(hour: f32) -> (f32, &'static str) {
    let p = crate::render::daylight::path();
    let at = |(h, n): (f32, &'static str)| (p.solar(h), n);
    PRESETS.iter().copied().map(at).find(|(h, _)| *h > hour + 0.05).unwrap_or(at(PRESETS[0]))
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The sun's elevation at `hour`, in degrees.
pub fn sun_elevation(hour: f32) -> f32 {
    sun_dir(hour).y.asin().to_degrees()
}

/// Game hours per real minute at `hour`. The rates are set by where the sun
/// is on the reference day (daylight `SunPath::REFERENCE`); this world's
/// hour is mapped to the reference hour that plays the same part, so every
/// world and season spends the same real minutes in each kind of light,
/// however high its sun climbs.
pub fn rate(hour: f32) -> f32 {
    let (r, slope) = crate::render::daylight::path().unsolar(hour);
    reference_rate(r) * slope
}

/// Hours per real minute on the reference day.
fn reference_rate(hour: f32) -> f32 {
    let e = crate::render::daylight::SunPath::REFERENCE.dir(hour).y.asin().to_degrees();
    let mut r = RATES[0];
    for (i, (edge, w)) in EDGES.iter().enumerate() {
        r += (RATES[i + 1] - RATES[i]) * smooth(edge - w, edge + w, e);
    }
    r
}

/// Advance the clock by `dt` real seconds.
pub fn advance(hour: f32, dt: f32) -> f32 {
    (hour + dt / 60.0 * rate(hour)).rem_euclid(24.0)
}

/// Any hour folded into 0..24 (worlds and the command line may give 24 or more).
pub fn wrap(hour: f32) -> f32 {
    hour.rem_euclid(24.0)
}

/// The hours the moon is the key light between: the sun 3 degrees down in
/// the evening and again in the morning (render/daylight.rs `apply`).
fn moon_hours() -> (f32, f32) {
    let p = crate::render::daylight::path();
    let (sl, cl) = p.latitude.to_radians().sin_cos();
    let (sd, cd) = p.declination.to_radians().sin_cos();
    // The hour angle at which the sun's elevation is -3 degrees.
    let c = (((-3f32).to_radians().sin() - sl * sd) / (cl * cd)).clamp(-1.0, 1.0);
    let h = c.acos() / std::f32::consts::PI * 12.0;
    (12.0 + h, 36.0 - h)
}

/// Real minutes from the evening's handover to the moon until `hour`
/// (0 outside the night), and the night's whole length in real minutes.
pub fn night_minutes(hour: f32) -> (f32, f32) {
    let (from, to) = moon_hours();
    let x = (hour - 12.0).rem_euclid(24.0) + 12.0;
    let minutes = |a: f32, b: f32| {
        const STEPS: usize = 160;
        let dh = (b - a) / STEPS as f32;
        (0..STEPS).map(|i| dh / rate(a + (i as f32 + 0.5) * dh)).sum::<f32>()
    };
    let total = minutes(from, to);
    let now = if x <= from {
        0.0
    } else if x >= to {
        total
    } else {
        minutes(from, x)
    };
    (now, total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_light_gets_its_share_of_a_25_minute_day() {
        let bands = [-90.0, -12.0, -6.0, -2.0, 2.0, 11.0, 32.0, 50.0, 90.0];
        let want = [15.0, 7.0, 3.0, 6.7, 15.1, 16.3, 17.0, 19.9];
        let mut got = [0.0f32; 8];
        let n = 24000;
        let dh = 24.0 / n as f32;
        for i in 0..n {
            let h = (i as f32 + 0.5) * dh;
            // The shares belong to the reference day the rates were fitted
            // on; every world's day is that one mapped through `solar`.
            let r = crate::render::daylight::path().unsolar(h).0;
            let e = crate::render::daylight::SunPath::REFERENCE.dir(r).y.asin().to_degrees();
            let b = bands.iter().rposition(|lo| *lo <= e).unwrap();
            got[b] += dh / rate(h);
        }
        let total: f32 = got.iter().sum();
        assert!((total - 25.0).abs() < 0.1, "a day takes {total} minutes");
        for (g, w) in got.iter().zip(want) {
            assert!((100.0 * g / total - w).abs() < 0.3, "{got:?}");
        }
    }

    #[test]
    fn the_clock_runs_round() {
        assert!((advance(23.99, 60.0) - (23.99 + rate(23.99) - 24.0)).abs() < 0.001);
        assert!(rate(0.0) > 2.0 && rate(crate::render::daylight::path().solar(17.5)) < 0.5);
        let (now, total) = night_minutes(0.0);
        assert!((5.5..6.5).contains(&total) && now > 0.3 * total && now < 0.7 * total);
        assert_eq!(night_minutes(12.0), (0.0, total));
        assert_eq!(next_preset(20.5).1, "NIGHT");
        assert_eq!(next_preset(23.5).1, "DAWN");
        assert_eq!(next_preset(11.0).1, "MIDDAY");
    }
}
