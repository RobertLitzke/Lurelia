//! A driver for any `Vehicle`: follows a closed track's centreline and
//! plans its speed from the bends ahead. It fills the same `Controls` the
//! keys do, so it drives a car exactly as a player could. Made for testing
//! a tune against a steady driver (`vehicle_laps` in mod.rs), and as the
//! start of race opponents.
//!
//! Steering is pure pursuit: aim at a point on the centreline a little way
//! ahead, further the faster the car goes, and turn the wheels by the angle
//! that arcs the car through it. Speed: every point ahead has a corner speed
//! from its curvature and a grip the driver trusts, and the car must be able
//! to brake down to it in the distance left; the lowest of those is the
//! target now.

use super::{Controls, Vehicle};
use glam::{Vec2, Vec3};

/// A closed loop on the ground: centreline points (x, z) in order, and how
/// wide the road is.
#[derive(Clone, Debug)]
pub struct Track {
    pub points: Vec<Vec2>,
    /// Distance along the loop to each point, metres; `length` closes it.
    pub along: Vec<f32>,
    pub length: f32,
    pub width: f32,
    /// Curvature at each point, 1/m, signed (positive bends left).
    pub curve: Vec<f32>,
    /// The road's height at each point, where known (`with_heights`):
    /// what tells a driver about crests and descents.
    pub heights: Vec<f32>,
    /// The road's width at each point where it varies (`with_widths`);
    /// empty: `width` all round.
    pub widths: Vec<f32>,
    /// Boost pads (`with_pads`): where each starts round the lap, its middle
    /// in metres right of the centreline, and its length.
    pub pads: Vec<(f32, f32, f32)>,
    /// Jumps (from, to round the lap): crests meant to be flown, which a
    /// driver doesn't slow for.
    pub jumps: Vec<(f32, f32)>,
}

/// Where a car is on the track.
#[derive(Clone, Copy, Debug, Default)]
pub struct Place {
    /// Index of the segment it is on.
    pub seg: usize,
    /// Distance along the loop, metres.
    pub s: f32,
    /// Distance off the centreline, metres; positive is to the right of the
    /// way round.
    pub off: f32,
}

impl Track {
    /// A track through `points`, which should be a metre or two apart.
    pub fn new(points: Vec<Vec2>, width: f32) -> Track {
        let n = points.len();
        let mut along = Vec::with_capacity(n);
        let mut s = 0.0;
        for i in 0..n {
            along.push(s);
            s += (points[(i + 1) % n] - points[i]).length();
        }
        // Curvature from the turn of the heading over a few points either
        // side, so a polyline's kinks don't read as hairpins.
        let k = 3;
        let curve = (0..n)
            .map(|i| {
                let a = points[(i + n - k) % n];
                let b = points[i];
                let c = points[(i + k) % n];
                let (d1, d2) = (b - a, c - b);
                let turn = d1.perp_dot(d2).atan2(d1.dot(d2));
                // In (x, z) a left turn is negative perp_dot (z runs south).
                -turn / ((d1.length() + d2.length()) * 0.5).max(0.01)
            })
            .collect();
        Track {
            points,
            along,
            length: s,
            width,
            curve,
            heights: Vec::new(),
            widths: Vec::new(),
            pads: Vec::new(),
            jumps: Vec::new(),
        }
    }

    /// A world's circuit as a race drives it: its heights, its widths and
    /// its boost pads, each pad's middle in metres off the centreline from
    /// its share of the way to the edge (as race_track.rs paints it).
    pub fn of(circuit: &crate::world::fields::Circuit) -> Track {
        let track = Track::new(circuit.points.clone(), circuit.width).with_heights(circuit.heights.clone()).with_widths(circuit.widths.clone());
        let pads = circuit.pads.iter().map(|p| (p.s, p.side.clamp(-1.0, 1.0) * (track.width_at(p.s + p.len * 0.5) * 0.5 - 2.0).max(0.0), p.len)).collect();
        let mut track = track.with_pads(pads);
        track.jumps = circuit.jumps.clone();
        track
    }

    /// Whether `s` round the lap is on a jump: its ramp, its lip or where
    /// karts fly and come down.
    pub fn on_jump(&self, s: f32) -> bool {
        let s = s.rem_euclid(self.length);
        self.jumps.iter().any(|&(a, b)| if a <= b { (a..b).contains(&s) } else { s >= a || s < b })
    }

    /// The same track with the road's width at each point; `width` becomes
    /// the widest.
    pub fn with_widths(mut self, widths: Vec<f32>) -> Track {
        if widths.len() == self.points.len() {
            self.width = widths.iter().copied().fold(0.0f32, f32::max);
            self.widths = widths;
        }
        self
    }

    /// The same track with boost pads: (start round the lap, metres right of
    /// the centreline, length).
    pub fn with_pads(mut self, pads: Vec<(f32, f32, f32)>) -> Track {
        self.pads = pads;
        self
    }

    /// The road's width `s` metres round.
    pub fn width_at(&self, s: f32) -> f32 {
        if self.widths.is_empty() {
            self.width
        } else {
            self.widths[self.index(s)]
        }
    }

    /// Whether a car at `place` is on a boost pad: within its length and
    /// 1.5 m of its middle.
    pub fn on_pad(&self, place: Place) -> bool {
        self.pads.iter().any(|&(s, side, len)| (place.s - s).rem_euclid(self.length) < len && (place.off - side).abs() < 1.5)
    }

    /// The same track, knowing the road's height at each point.
    pub fn with_heights(mut self, heights: Vec<f32>) -> Track {
        if heights.len() == self.points.len() {
            self.heights = heights;
        }
        self
    }

    fn index(&self, s: f32) -> usize {
        let s = s.rem_euclid(self.length);
        match self.along.binary_search_by(|a| a.partial_cmp(&s).unwrap()) {
            Ok(i) => i,
            Err(i) => i - 1,
        }
    }

    /// The road's grade `s` metres round (rise per metre, negative going
    /// down) and its vertical curvature over a few metres (1/m, negative
    /// over a crest). Both 0 where the heights aren't known.
    pub fn grade_at(&self, s: f32) -> (f32, f32) {
        if self.heights.is_empty() {
            return (0.0, 0.0);
        }
        let n = self.points.len();
        let i = self.index(s);
        let k = 6;
        let (a, b, c) = (self.heights[(i + n - k) % n], self.heights[i], self.heights[(i + k) % n]);
        let gap = k as f32 * (self.length / n as f32);
        ((c - a) / (2.0 * gap), (a - 2.0 * b + c) / (gap * gap))
    }

    /// The road's height `s` metres round, where the heights are known.
    pub fn height_at(&self, s: f32) -> Option<f32> {
        (!self.heights.is_empty()).then(|| self.heights[self.index(s)])
    }

    /// The point `s` metres round the loop, and the direction there.
    pub fn at(&self, s: f32) -> (Vec2, Vec2) {
        let s = s.rem_euclid(self.length);
        let n = self.points.len();
        let i = match self.along.binary_search_by(|a| a.partial_cmp(&s).unwrap()) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        let (a, b) = (self.points[i], self.points[(i + 1) % n]);
        let seg = (b - a).length().max(1e-4);
        let t = (s - self.along[i]) / seg;
        (a.lerp(b, t), (b - a) / seg)
    }

    /// The curvature `s` metres round, 1/m, signed.
    pub fn curve_at(&self, s: f32) -> f32 {
        let s = s.rem_euclid(self.length);
        let i = match self.along.binary_search_by(|a| a.partial_cmp(&s).unwrap()) {
            Ok(i) => i,
            Err(i) => i - 1,
        };
        self.curve[i]
    }

    /// Where `p` is, searching near `hint` (the last place) first; the
    /// whole loop when `hint` is None. Height counts where the road's heights
    /// are known, so a car at a crossing is placed on its own stretch.
    pub fn place(&self, p: Vec3, hint: Option<Place>) -> Place {
        let q = Vec2::new(p.x, p.z);
        let n = self.points.len();
        let range: Box<dyn Iterator<Item = usize>> = match hint {
            Some(h) => Box::new((0..60).map(move |k| (h.seg + n + k - 20) % n)),
            None => Box::new(0..n),
        };
        let mut best = (f32::MAX, Place::default());
        for i in range {
            let (a, b) = (self.points[i], self.points[(i + 1) % n]);
            let d = b - a;
            let len = d.length().max(1e-4);
            let t = ((q - a).dot(d) / (len * len)).clamp(0.0, 1.0);
            let on = a + d * t;
            let dist = (q - on).length_squared();
            // Where the road's heights are known, a stretch far above or
            // below the car counts as further away: at a crossing the car is
            // on the stretch at its own height, not the one over or under it.
            let rise = self.heights.get(i).map_or(0.0, |h| ((p.y - h).abs() - 2.0).max(0.0));
            let score = dist + rise * rise * 4.0;
            if score < best.0 {
                let side = d.perp_dot(q - a);
                // perp_dot positive is to the right of the way round in (x, z).
                best = (
                    score,
                    Place {
                        seg: i,
                        s: self.along[i] + t * len,
                        off: dist.sqrt() * side.signum(),
                    },
                );
            }
        }
        best.1
    }
}

/// How the bot drives. The defaults are the steady driver the tune is
/// tested against; a race could vary them per opponent.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Look-ahead: seconds of travel, plus metres, within a range.
    pub look_secs: f32,
    pub look_add: f32,
    pub look_min: f32,
    pub look_max: f32,
    /// Sideways grip the driver trusts in a bend, m/s².
    pub corner_g: f32,
    /// Braking it trusts, m/s².
    pub brake_g: f32,
    /// How far ahead it reads the bends, metres.
    pub plan: f32,
    /// Overtaking: how far ahead a car in its lane makes it move over,
    /// metres, and how far to the side it passes, metres.
    pub pass_look: f32,
    pub pass_side: f32,
    /// Over to a boost pad coming up when nobody is to be passed.
    pub pads: bool,
    /// Drift through bends at least this tight, 1/m (0 never): hop into the
    /// bend and let go for the boost as it opens (`Driver::drift`).
    pub drift: f32,
    /// Sit in the slipstream of a kart close ahead on a straight before
    /// pulling out to pass.
    pub draft: bool,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            look_secs: 0.5,
            look_add: 5.0,
            look_min: 7.0,
            look_max: 28.0,
            corner_g: 14.0,
            brake_g: 12.0,
            plan: 120.0,
            pass_look: 16.0,
            pass_side: 3.2,
            pads: true,
            drift: 0.0,
            draft: false,
        }
    }
}

/// Another car on the track, as a driver sees it.
#[derive(Clone, Copy, Debug)]
pub struct Rival {
    pub place: Place,
    /// Speed along the track, m/s.
    pub speed: f32,
}

impl Style {
    /// A racer: what two 20-round tournaments (`vehicle_tournament` in
    /// mod.rs, seeds 0x9e3779b97f4a7c15 and 123456789) bred from the default,
    /// about two seconds a race quicker on the test circuit in the beach
    /// buggy. It trusts more grip in the bends and brakes harder; the
    /// default stays the steady driver the lap test measures tunes against.
    pub fn racer() -> Style {
        Style {
            corner_g: 16.8,
            brake_g: 13.0,
            look_secs: 0.48,
            look_add: 4.7,
            pass_look: 15.0,
            pass_side: 3.0,
            ..Style::default()
        }
    }
}

impl Style {
    /// A kart racer: what a 20-round tournament bred from the default in
    /// the kart on the test circuit made smooth (`BUMPS=0 TUNE=kart
    /// vehicle_tournament`). Next to `racer` it looks further ahead and
    /// brakes a little less hard.
    pub fn kart() -> Style {
        Style {
            corner_g: 16.4,
            brake_g: 12.3,
            look_secs: 0.63,
            look_add: 4.9,
            pass_look: 16.0,
            pass_side: 3.0,
            ..Style::default()
        }
    }
}

impl Style {
    /// A kart driver fitted to the kart it drives:
    /// trusting `SHARE` of the sideways grip its tyres give at 22 m/s
    /// (`kart::reach`), and braking as `Style::kart` does, scaled by its grip
    /// against the plain kart's. So a heavier kart's driver brakes sooner and
    /// a grippier one corners faster, without a style bred for each.
    pub fn for_kart(spec: &super::Spec) -> Style {
        const SHARE: f32 = 0.8;
        /// The grip of the reference kart (library/vehicles/kart.json) that
        /// `Style::kart()` was tuned on.
        const KART_GRIP: f32 = 1.6;
        let k = Style::kart();
        Style { corner_g: SHARE * super::kart::reach(spec, 22.0), brake_g: k.brake_g * spec.grip / KART_GRIP, ..k }
    }
}

/// A bot at the wheel of one car.
#[derive(Clone, Debug, Default)]
pub struct Driver {
    pub style: Style,
    pub place: Option<Place>,
    /// The speed it wants now, m/s, for the HUD and the test.
    pub want: f32,
    /// How far right of the centreline it is driving, metres; it moves over
    /// to pass.
    pub lane: f32,
    /// Where the race would have it drive when nobody is to be passed
    /// (metres right of the centreline, over to an item box), set before
    /// each step.
    pub goal: Option<f32>,
    /// In a drift it started: seconds in it.
    pub drifting: Option<f32>,
}

impl Driver {
    pub fn new(style: Style) -> Driver {
        Driver { style, ..Default::default() }
    }

    /// The controls for this step, alone on the track (the lap test's).
    #[cfg(test)]
    pub fn drive(&mut self, car: &Vehicle, track: &Track) -> Controls {
        self.race(car, track, &[])
    }

    /// The controls for this step among `rivals`: a car ahead in its lane
    /// makes it move over to the side with more road, and if it can't get
    /// by, hold back to that car's speed rather than drive into it.
    pub fn race(&mut self, car: &Vehicle, track: &Track, rivals: &[Rival]) -> Controls {
        let st = self.style;
        let place = track.place(car.pos(), self.place);
        self.place = Some(place);
        let v = car.v_fwd();
        let half = track.width_at(place.s) * 0.5 - 1.5;

        let mut lane_to = 0.0f32;
        let mut hold: Option<f32> = None;
        let ahead = |r: &Rival| (r.place.s - place.s).rem_euclid(track.length);
        // Drafting: on a straight, a kart 4 to 12 m ahead going about as fast
        // is followed in its lane (its slipstream, race/feel.rs), and only
        // passed once it is close.
        let straight = (0..6).all(|k| track.curve_at(place.s + k as f32 * 8.0).abs() < 1.0 / 80.0);
        let tow = rivals.iter().filter(|r| st.draft && straight && r.speed > 12.0 && (4.0..12.0).contains(&ahead(r)) && (r.place.off - place.off).abs() < 3.0).min_by(|a, b| ahead(a).total_cmp(&ahead(b)));
        if let Some(r) = tow {
            lane_to = r.place.off;
        } else if let Some(r) = rivals
            .iter()
            .filter(|r| ahead(r) < st.pass_look && (r.place.off - self.lane).abs() < 2.4)
            .min_by(|a, b| ahead(a).total_cmp(&ahead(b)))
        {
            // Pass on the side with more road, unless that runs out.
            let right = r.place.off + st.pass_side;
            let left = r.place.off - st.pass_side;
            lane_to = if (half - r.place.off) >= (r.place.off + half) { right } else { left };
            if ahead(r) < 7.0 && r.speed < v {
                hold = Some(r.speed + 1.0);
            }
        } else if let Some(g) = self.goal {
            // Nobody to pass: where the race wants it (an item box).
            lane_to = g;
        } else if let Some(&(_, side, _)) = track.pads.iter().filter(|_| st.pads).find(|p| (2.0..45.0).contains(&(p.0 - place.s).rem_euclid(track.length))) {
            // Nobody to pass: over to the boost pad coming up.
            lane_to = side;
        }
        let lane_to = lane_to.clamp(-half, half);
        self.lane += (lane_to - self.lane).clamp(-0.06, 0.06);

        // Steering: pure pursuit toward a point ahead on its lane. Through
        // bends the point comes closer, so an S is followed rather than cut
        // across.
        let look = (v.abs() * st.look_secs + st.look_add).clamp(st.look_min, st.look_max);
        let bendy = (0..4).map(|k| track.curve_at(place.s + look * k as f32 / 3.0).abs()).fold(0.0f32, f32::max);
        let look = (look / (1.0 + 10.0 * bendy)).max(st.look_min * 0.7);
        let (aim, dir) = track.at(place.s + look);
        let aim = aim + Vec2::new(-dir.y, dir.x) * self.lane;
        let local = car.b.rot.inverse() * (Vec3::new(aim.x, car.pos().y, aim.y) - car.pos());
        let alpha = local.x.atan2(-local.z);
        let dist = Vec2::new(local.x, local.z).length().max(1.0);
        let wheel = (2.0 * car.spec.wheelbase * alpha.sin() / dist).atan();
        // Facing away from the way round (spun, or knocked about): full lock
        // toward it, rather than the little a pure pursuit gives behind.
        let steer = if alpha.abs() > std::f32::consts::FRAC_PI_2 { alpha.signum() } else { (wheel / car.lock().max(0.05)).clamp(-1.0, 1.0) };

        // Speed: the lowest of what each bend ahead allows, braking to it.
        // Over a crest the car goes light, so it is taken no faster than
        // keeps it on the ground; downhill the brakes stop it less.
        let g = car.spec.gravity;
        let mut want = car.spec.top_speed + 5.0;
        let mut d = 0.0;
        while d < st.plan {
            let k = track.curve_at(place.s + d).abs().max(1e-4);
            let (grade, bend) = track.grade_at(place.s + d);
            let mut limit = (st.corner_g / k).sqrt();
            if bend < -1e-4 && !track.on_jump(place.s + d) {
                limit = limit.min((0.6 * g / -bend).sqrt());
            }
            let brake = (st.brake_g + g * grade.min(0.0)).max(4.0);
            want = want.min((limit * limit + 2.0 * brake * d).sqrt());
            d += 2.0;
        }
        if let Some(h) = hold {
            want = want.min(h);
        }
        self.want = want;
        let gap = want - v;
        let (throttle, brake) = if gap > 1.0 {
            (1.0, 0.0)
        } else if gap > -0.5 {
            ((gap + 0.5) / 1.5, 0.0)
        } else {
            (0.0, ((-gap - 0.5) / 3.0).clamp(0.0, 1.0))
        };
        let hop = self.drift(car, track, place, look, steer, brake);
        let steer = match (self.drifting, car.drift) {
            // In a drift the keys say how tight: into the turn to tighten it,
            // out of it to open the line, as the pursuit asks.
            (Some(_), Some(d)) => d.dir * ((steer * d.dir - 0.45) * 2.5).clamp(-1.0, 1.0),
            _ => steer,
        };
        Controls {
            throttle,
            brake,
            steer,
            hop,
            ..Default::default()
        }
    }

    /// Whether to hold the hop (a kart's drift) this step. A drift starts
    /// in a bend at least `Style::drift` tight that runs on for a second or
    /// more, at speed and not braking, steered the bend's way; it is let go
    /// as the bend opens out ahead, once it has charged a tier (or if it has
    /// gone wrong: the kart off its line or slowed right down).
    fn drift(&mut self, car: &Vehicle, track: &Track, place: Place, look: f32, steer: f32, brake: f32) -> bool {
        let k_min = self.style.drift;
        if k_min <= 0.0 || car.spec.drift_boost <= 0.0 {
            self.drifting = None;
            return false;
        }
        let v = car.v_fwd();
        let dt = crate::game::STEP;
        let k_now = track.curve_at(place.s + look * 0.3);
        let half = track.width_at(place.s) * 0.5;
        match self.drifting {
            None => {
                // A bend this tight, for at least a second of travel ahead.
                let run = (0..6).all(|i| {
                    let k = track.curve_at(place.s + v * i as f32 * 0.2);
                    k.abs() > k_min && k.signum() == k_now.signum()
                });
                // Right is positive steer; a left bend is positive curvature.
                let way = -k_now.signum();
                let go = run && v > 12.0 && brake <= 0.0 && steer * way > 0.3 && car.drift.is_none() && place.off.abs() < half - 1.0;
                if go {
                    self.drifting = Some(0.0);
                }
                go
            }
            Some(t) => {
                let t = t + dt;
                self.drifting = Some(t);
                let charged = car.drift_tier() >= 1;
                let opens = track.curve_at(place.s + v * 0.35).abs() < k_min * 0.6;
                let wrong = place.off.abs() > half - 0.6 || v < 9.0 || t > 4.0 || (t > 0.2 && car.drift.is_none());
                let hold = !(wrong || (charged && opens));
                if !hold {
                    self.drifting = None;
                }
                hold
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn circle(r: f32) -> Track {
        let n = 200;
        let pts = (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                // Anticlockwise seen from above in (x, z), which is a left turn
                // with z running south.
                Vec2::new(r * a.cos(), -r * a.sin())
            })
            .collect();
        Track::new(pts, 10.0)
    }

    #[test]
    fn a_circle_has_the_curvature_of_its_radius() {
        let t = circle(50.0);
        assert!((t.length - std::f32::consts::TAU * 50.0).abs() < 0.5, "{}", t.length);
        for k in &t.curve {
            assert!((k - 0.02).abs() < 0.002, "curvature {k}");
        }
    }

    #[test]
    fn place_finds_the_side_and_distance() {
        let t = circle(50.0);
        // Driving anticlockwise, the outside of the circle is on the right.
        let p = t.place(Vec3::new(55.0, 0.0, 0.0), None);
        assert!((p.off - 5.0).abs() < 0.1, "{}", p.off);
        let p = t.place(Vec3::new(45.0, 0.0, 0.0), None);
        assert!((p.off + 5.0).abs() < 0.1, "{}", p.off);
    }

    /// A figure-eight with the second loop on a bridge over the first where
    /// they cross: a car driven round it on the road is placed on its own
    /// stretch all the way, so its distance round only ever grows, through
    /// the crossing too, and from a cold start at the crossing on either
    /// level.
    #[test]
    fn a_crossing_is_placed_by_height() {
        let n = 400;
        let (mut pts, mut hs) = (Vec::new(), Vec::new());
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            // A lemniscate: it crosses itself at the origin, at a = pi/2 and
            // a = 3 pi/2.
            let (s, c) = a.sin_cos();
            let k = 60.0 / (1.0 + s * s);
            pts.push(Vec2::new(k * c, k * s * c));
            // Up to 8 m over the first crossing, level over the second.
            hs.push(10.0 + 8.0 * (a - std::f32::consts::FRAC_PI_2).cos().max(0.0).powi(2));
        }
        let t = Track::new(pts.clone(), 10.0).with_heights(hs.clone());
        let (mut last, mut hint) = (None::<f32>, None);
        for i in 0..n {
            let p = Vec3::new(pts[i].x, hs[i] + 0.3, pts[i].y);
            let pl = t.place(p, hint);
            if let Some(l) = last {
                let ds = (pl.s - l).rem_euclid(t.length);
                assert!(ds < 3.0, "jumped {ds} m round at point {i}");
            }
            last = Some(pl.s);
            hint = Some(pl);
        }
        // Cold, at the crossing: on the bridge, and under it.
        let up = t.place(Vec3::new(0.0, 18.3, 0.0), None);
        let down = t.place(Vec3::new(0.0, 10.3, 0.0), None);
        let near = |s: f32, want: f32| ((s - want).rem_euclid(t.length)).min((want - s).rem_euclid(t.length)) < 5.0;
        assert!(near(up.s, t.along[n / 4]), "on the bridge placed at {} not {}", up.s, t.along[n / 4]);
        assert!(near(down.s, t.along[3 * n / 4]), "under it placed at {} not {}", down.s, t.along[3 * n / 4]);
    }
}
