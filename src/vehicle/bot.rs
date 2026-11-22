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
//!
//! Nothing in the game drives with it yet (the lap test does), so it is
//! kept from the dead-code warning until a race does.
#![allow(dead_code)]

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
        }
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
    /// whole loop when `hint` is None.
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
            if dist < best.0 {
                let side = d.perp_dot(q - a);
                // perp_dot positive is to the right of the way round in (x, z).
                best = (
                    dist,
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
}

impl Driver {
    pub fn new(style: Style) -> Driver {
        Driver { style, ..Default::default() }
    }

    /// The controls for this step, alone on the track.
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
        let half = track.width * 0.5 - 1.5;

        let mut lane_to = 0.0f32;
        let mut hold: Option<f32> = None;
        let ahead = |r: &Rival| (r.place.s - place.s).rem_euclid(track.length);
        if let Some(r) = rivals
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
            if bend < -1e-4 {
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
        Controls {
            throttle,
            brake,
            steer,
            ..Default::default()
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
}
