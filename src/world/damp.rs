//! What the weather leaves on the ground: wet, standing water, frost and the
//! fog that lies in the valleys after a clear night.
//!
//! The weather (weather.rs) is what is falling now; this is what it has left
//! behind, so it runs on the history rather than the moment. A shower leaves
//! the ground dark for hours after the sky has cleared, a clear calm night in
//! a cold season leaves frost that the sun takes off again, and a still night
//! over damp ground fills the hollows with fog by dawn.
//!
//! Like lying snow it is one state for the whole world, a few numbers stepped
//! on the game clock. Where on the land each of them shows is the shaders'
//! business (scene.wgsl `damp_ground`, foliage.wgsl): puddles stand on level
//! paths and bare ground, wet darkens what faces up, and frost holds on in
//! the shade after the sun has taken it off everything it reaches. That last
//! is why frost is two numbers: how much is left where the sun has been, and
//! how much where it has not.

use crate::world::weather::Sky;

/// Wet beyond this is ground that cannot take more: the rest stands.
pub const SOAKED: f32 = 1.0;

/// The conditions for one step: the sky, the sun's height and how cold the
/// time of year is.
#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub sky: Sky,
    /// The sine of the real sun's elevation: below 0 at night.
    pub sun_y: f32,
    /// How readily frost forms, 0 (never: summer, or a hot country) to 1 (a
    /// winter night).
    pub cold: f32,
    /// Lying snow that went this step (season.rs `lying`), in its own units:
    /// melt water, which wets the ground like rain.
    pub melt: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Damp {
    /// How wet the ground is, 0 (dry) to `SOAKED`.
    pub wet: f32,
    /// Standing water, 0 (none) to 1 (every hollow full).
    pub pools: f32,
    /// Frost where the sun has not reached since it rose, 0 to 1.
    pub frost_shade: f32,
    /// Frost where the sun has been, 0 to 1. Never more than in the shade.
    pub frost_sun: f32,
    /// Fog lying in the hollows, 0 to 1.
    pub ground_fog: f32,
}

/// How clear the night sky is, 0 to 1: frost and valley fog both need the
/// ground to lose its warmth to an open sky, and a cloud deck stops that.
fn clear_of(sky: &Sky) -> f32 {
    let c = ((1.0 - sky.overcast) * (1.0 - sky.rain) * (1.0 - sky.cover.max(0.0))).clamp(0.0, 1.0);
    c * c
}

/// How still the air is, 0 to 1: a fair day's breeze (wind 1) is still
/// enough, the stiffer wind of a cloudy day is not.
fn calm_of(sky: &Sky) -> f32 {
    (1.0 - (sky.wind - 1.0) / 0.25).clamp(0.0, 1.0)
}

impl Damp {
    /// The ground a world opens on under `sky`: soaked in rain, dry in sun,
    /// and nothing that needs a night to make.
    pub fn settled(sky: &Sky) -> Damp {
        let wet = (sky.rain * 1.4 + sky.mist * 0.3).min(SOAKED);
        Damp {
            wet,
            pools: (sky.rain - 0.3).max(0.0),
            ..Damp::default()
        }
    }

    /// The VG_DAMP dev hook: `wet,pools,frost_shade,frost_sun,fog`, any
    /// left off kept as they are, for judging a look in a headless shot.
    pub fn with_override(mut self, spec: Option<&str>) -> Damp {
        let Some(spec) = spec else {
            return self;
        };
        let fields = [&mut self.wet, &mut self.pools, &mut self.frost_shade, &mut self.frost_sun, &mut self.ground_fog];
        for (f, v) in fields.into_iter().zip(spec.split(',')) {
            if let Ok(v) = v.trim().parse::<f32>() {
                *f = v.clamp(0.0, 1.0);
            }
        }
        self
    }

    /// Carry the ground forward by `hours` of game clock.
    pub fn advance(&mut self, hours: f32, c: &Conditions) {
        if hours <= 0.0 {
            return;
        }
        let sky = &c.sky;
        let day = c.sun_y.max(0.0);
        let night = c.sun_y < 0.05;
        let clear = clear_of(sky);
        let calm = calm_of(sky);

        // Wet. Rain and melt water put it on; sun, wind and heat take it off,
        // but only once the rain has stopped. A night with a clear sky and no
        // wind brings dew, and a foggy day keeps the ground damp.
        let falling = sky.rain;
        let gain = falling * 1.1 + c.melt * 0.8 + sky.mist * 0.12 + if night { 0.08 * clear * calm } else { 0.0 };
        let dry = if falling > 0.1 {
            0.0
        } else {
            0.03 + day * 0.42 * (1.0 - 0.75 * sky.overcast) + (sky.wind - 1.0).max(0.0) * 0.12 + sky.heat * 0.3
        };
        // Dew and fog wet the ground only so far: a dewy morning is damp, not
        // soaked.
        let cap = if falling > 0.05 || c.melt > 0.0 { SOAKED } else { SOAKED.min(self.wet.max(0.35)) };
        self.wet = (self.wet + gain * hours).min(cap.max(self.wet));
        self.wet = (self.wet - dry * hours).clamp(0.0, SOAKED);

        // Standing water fills once the ground is near soaked and goes slower
        // than the wet does: puddles outlast the shower by an afternoon.
        let fill = if self.wet > 0.8 { falling * 0.7 + c.melt * 0.4 } else { 0.0 };
        let drain = if falling > 0.1 { 0.0 } else { 0.06 + dry * 0.35 };
        self.pools = (self.pools + (fill - drain) * hours).clamp(0.0, 1.0);

        // Frost. It forms on a clear, still night in a cold season, as far
        // as the season allows. After sunrise the sun takes it off whatever
        // it reaches within the hour; in shade it goes with the warming air,
        // slowly on a winter morning. Rain takes it off everywhere.
        if night && sky.heat < 0.1 {
            let form = 0.3 * c.cold * clear * calm;
            let cap = c.cold;
            for f in [&mut self.frost_shade, &mut self.frost_sun] {
                if *f < cap {
                    *f = (*f + form * hours).min(cap);
                }
            }
        }
        let thaw_sun = if day > 0.0 { 0.4 + day * 6.0 } else { 0.0 };
        let thaw_shade = if day > 0.0 { 0.08 + day * 0.6 * (1.3 - c.cold) } else { 0.0 };
        let washed = falling * 2.0 + sky.heat;
        self.frost_sun = (self.frost_sun - (thaw_sun + washed) * hours).max(0.0);
        self.frost_shade = (self.frost_shade - (thaw_shade + washed) * hours).max(0.0);
        self.frost_sun = self.frost_sun.min(self.frost_shade);

        // Fog in the hollows: a clear, calm night over ground with water in
        // it. It thickens through the small hours and the sun burns it off
        // within a couple of hours of rising; any wind clears it at once.
        let want = if night { 0.75 * clear * calm * (0.25 + self.wet).min(1.0) } else { 0.0 };
        if self.ground_fog < want {
            self.ground_fog = (self.ground_fog + 0.12 * hours).min(want);
        } else {
            let burn = day * 1.6 + (1.0 - calm) * 1.5 + if night { 0.08 } else { 0.0 };
            self.ground_fog = (self.ground_fog - burn * hours).max(want).max(0.0);
        }
    }

    /// The frame's `damp` globals: wet, standing water, frost in the shade,
    /// frost in the sun.
    pub fn uniform(&self) -> [f32; 4] {
        [self.wet.min(1.0), self.pools, self.frost_shade, self.frost_sun]
    }

    /// Mist added everywhere for the fog in the hollows (render/daylight.rs
    /// `Air::mist_floor`). The mist already lies thickest below the smoothed
    /// ground, so more of it everywhere fills the valleys and leaves the
    /// ridges in the clear.
    pub fn mist(&self) -> f32 {
        self.ground_fog * 0.45
    }

    /// How soft the ground underfoot is from the wet, 0 to 1: what lets bare
    /// earth take a print (creatures/sign.rs).
    pub fn soft(&self) -> f32 {
        (self.wet * 1.2).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::weather::Kind;

    fn run(d: &mut Damp, kind: Kind, hours: f32, sun_y: f32, cold: f32) {
        let c = Conditions {
            sky: kind.sky(),
            sun_y,
            cold,
            melt: 0.0,
        };
        for _ in 0..(hours * 60.0) as usize {
            d.advance(1.0 / 60.0, &c);
        }
    }

    #[test]
    fn rain_soaks_and_the_sun_dries_it_slower_than_it_fell() {
        let mut d = Damp::default();
        run(&mut d, Kind::Rain, 2.0, 0.6, 0.0);
        assert!(d.wet > 0.95, "two hours of rain left wet {}", d.wet);
        assert!(d.pools > 0.3, "and pools {}", d.pools);
        run(&mut d, Kind::Sun, 1.0, 0.6, 0.0);
        assert!(d.wet > 0.4, "an hour of sun dried it to {}", d.wet);
        let pools = d.pools;
        run(&mut d, Kind::Sun, 4.0, 0.6, 0.0);
        assert!(d.wet < 0.05, "an afternoon of sun left wet {}", d.wet);
        assert!(d.pools < pools);
    }

    #[test]
    fn a_dewy_night_is_damp_not_soaked() {
        let mut d = Damp::default();
        run(&mut d, Kind::Sun, 10.0, -0.4, 0.0);
        assert!(d.wet > 0.2 && d.wet <= 0.35 + 1e-4, "dew left {}", d.wet);
        assert_eq!(d.pools, 0.0);
    }

    #[test]
    fn frost_forms_on_a_clear_winter_night_and_lasts_longer_in_shade() {
        let mut d = Damp::default();
        run(&mut d, Kind::Sun, 6.0, -0.4, 1.0);
        assert!(d.frost_shade > 0.9 && d.frost_sun > 0.9);
        run(&mut d, Kind::Sun, 0.75, 0.15, 1.0);
        assert!(d.frost_sun < 0.2, "the sun left {}", d.frost_sun);
        assert!(d.frost_shade > 0.6, "the shade kept only {}", d.frost_shade);
    }

    #[test]
    fn no_frost_in_summer_under_cloud_or_in_wind() {
        for (kind, cold) in [(Kind::Sun, 0.0), (Kind::Clouds, 1.0), (Kind::Rain, 1.0)] {
            let mut d = Damp::default();
            run(&mut d, kind, 8.0, -0.4, cold);
            assert!(d.frost_shade < 0.05, "{kind:?} at cold {cold} froze {}", d.frost_shade);
        }
    }

    #[test]
    fn a_still_night_fills_the_valleys_and_the_morning_clears_them() {
        let mut d = Damp { wet: 0.6, ..Damp::default() };
        run(&mut d, Kind::Sun, 6.0, -0.4, 0.0);
        assert!(d.ground_fog > 0.5, "fog {}", d.ground_fog);
        run(&mut d, Kind::Sun, 2.0, 0.4, 0.0);
        assert!(d.ground_fog < 0.05, "still {} after two hours of sun", d.ground_fog);
    }
}
