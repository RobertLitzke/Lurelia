//! The building blocks every voice is made of: noise, filters, smoothers, a
//! pool of short events, a bank of bubbles and the reverb. Nothing here knows
//! what a wind or a river is, and nothing allocates once it is made, because
//! all of it runs inside the audio callback.

use std::f32::consts::{PI, TAU};

/// A small fast generator: xorshift32. Seeded per voice and per channel, so
/// the left and right ears hear independent noise (a bed that is the same in
/// both ears sits in the middle of the head).
#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9) | 1)
    }

    #[inline]
    pub fn next(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0 to 1.
    #[inline]
    pub fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 * (1.0 / 16_777_216.0)
    }

    /// -1 to 1: white noise.
    #[inline]
    pub fn bi(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }

    pub fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.unit()
    }
}

/// Pink noise (-3 dB an octave), Paul Kellet's refined filter: seven one-pole
/// stages over white noise. Comes out at roughly the level of the white.
#[derive(Clone)]
pub struct Pink {
    rng: Rng,
    b: [f32; 7],
}

impl Pink {
    pub fn new(seed: u32) -> Pink {
        Pink { rng: Rng::new(seed), b: [0.0; 7] }
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        let w = self.rng.bi();
        let b = &mut self.b;
        b[0] = 0.99886 * b[0] + w * 0.0555179;
        b[1] = 0.99332 * b[1] + w * 0.0750759;
        b[2] = 0.96900 * b[2] + w * 0.153852;
        b[3] = 0.86650 * b[3] + w * 0.3104856;
        b[4] = 0.55000 * b[4] + w * 0.5329522;
        b[5] = -0.7616 * b[5] - w * 0.0168980;
        let out = b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + w * 0.5362;
        b[6] = w * 0.115926;
        out * 0.2
    }
}

/// Brown noise (-6 dB an octave): white noise through a leaky integrator.
#[derive(Clone)]
pub struct Brown {
    rng: Rng,
    y: f32,
}

impl Brown {
    pub fn new(seed: u32) -> Brown {
        Brown { rng: Rng::new(seed), y: 0.0 }
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        self.y = (self.y + 0.02 * self.rng.bi()) / 1.02;
        self.y * 3.5
    }
}

/// A one-pole lowpass. Cheap enough to use anywhere a slope of 6 dB an
/// octave will do.
#[derive(Clone, Default)]
pub struct OnePole {
    a: f32,
    y: f32,
}

impl OnePole {
    pub fn set(&mut self, hz: f32, rate: f32) {
        self.a = 1.0 - (-TAU * hz.max(1.0) / rate).exp();
    }

    #[inline]
    pub fn lp(&mut self, x: f32) -> f32 {
        self.y += self.a * (x - self.y);
        self.y
    }
}

/// A two-pole filter (RBJ cookbook coefficients, transposed direct form II).
/// Coefficients are recomputed once a block, never a sample.
#[derive(Clone, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn coeffs(&mut self, b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) {
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn w(hz: f32, q: f32, rate: f32) -> (f32, f32) {
        let w0 = TAU * hz.clamp(10.0, rate * 0.45) / rate;
        let alpha = w0.sin() / (2.0 * q.max(0.05));
        (w0.cos(), alpha)
    }

    /// Bandpass with a peak gain of 1 at `hz`.
    pub fn bandpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, al) = Self::w(hz, q, rate);
        self.coeffs(al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    pub fn lowpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, al) = Self::w(hz, q, rate);
        self.coeffs((1.0 - c) * 0.5, 1.0 - c, (1.0 - c) * 0.5, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    pub fn highpass(&mut self, hz: f32, q: f32, rate: f32) {
        let (c, al) = Self::w(hz, q, rate);
        self.coeffs((1.0 + c) * 0.5, -(1.0 + c), (1.0 + c) * 0.5, 1.0 + al, -2.0 * c, 1.0 - al);
    }

    #[inline]
    pub fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// A value that glides toward where it is told to go, so nothing a game
/// frame does ever steps in the output. `secs` is the time to cover about
/// two thirds of a change; it is stepped once a block.
#[derive(Clone, Copy)]
pub struct Glide {
    pub v: f32,
    pub to: f32,
    k: f32,
}

impl Glide {
    pub fn new(v: f32, secs: f32, block_rate: f32) -> Glide {
        Glide { v, to: v, k: 1.0 - (-1.0 / (secs.max(1e-3) * block_rate)).exp() }
    }

    #[inline]
    pub fn step(&mut self) -> f32 {
        self.v += (self.to - self.v) * self.k;
        if (self.to - self.v).abs() < 1e-6 {
            self.v = self.to;
        }
        self.v
    }
}

/// A slow random wander, 0 to 1: random values `1 / hz` seconds apart, eased
/// from one to the next. Three of these at unrelated rates make a gust
/// signal with no period an ear can latch onto.
#[derive(Clone)]
pub struct Wander {
    rng: Rng,
    a: f32,
    b: f32,
    t: f32,
    dt: f32,
}

impl Wander {
    pub fn new(seed: u32, hz: f32, block_rate: f32) -> Wander {
        let mut rng = Rng::new(seed);
        let a = rng.unit();
        let b = rng.unit();
        Wander { rng, a, b, t: 0.0, dt: hz / block_rate }
    }

    /// Advance one block.
    pub fn step(&mut self) -> f32 {
        self.t += self.dt;
        if self.t >= 1.0 {
            self.t -= 1.0;
            self.a = self.b;
            self.b = self.rng.unit();
        }
        let e = 0.5 - 0.5 * (PI * self.t).cos();
        self.a + (self.b - self.a) * e
    }
}

/// Equal-power gains for a pan of -1 (left) to 1 (right).
#[inline]
pub fn pan(p: f32) -> (f32, f32) {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * (PI / 4.0);
    (a.cos(), a.sin())
}

/// A crowd of short events -- leaf slaps, raindrops, drips -- each a burst
/// of noise with an exponential tail, rung through a few shared resonators.
/// This is PhISEM's idea (Cook 1997): model the collisions by how often they
/// happen and how they decay, not one at a time. A burst goes to one of the
/// resonators at random, so the crowd has several pitches in it rather than
/// one.
pub struct Grains {
    rng: Rng,
    /// Active events: envelope, decay per sample, left and right weight,
    /// which resonator, and the envelope it is let go at (a thousandth of
    /// where it started).
    live: [(f32, f32, f32, f32, u8, f32); Grains::MAX],
    n: usize,
    bands: [(Biquad, Biquad); Grains::BANDS],
    /// Expected events per sample.
    pub rate: f32,
}

/// How a crowd of events sounds: resonator centres (spread between `lo` and
/// `hi` Hz), their sharpness, and the tail of each event in seconds.
#[derive(Clone, Copy, Debug)]
pub struct GrainShape {
    pub lo: f32,
    pub hi: f32,
    pub q: f32,
    pub decay: [f32; 2],
    /// Where the events are centred, -1 (left) to 1 (right), and how widely
    /// they spread about it, 0 (a point) to 1 (all round).
    pub centre: f32,
    pub width: f32,
}

impl Grains {
    pub const MAX: usize = 96;
    const BANDS: usize = 4;

    pub fn new(seed: u32) -> Grains {
        Grains {
            rng: Rng::new(seed),
            live: [(0.0, 0.0, 0.0, 0.0, 0, 0.0); Grains::MAX],
            n: 0,
            bands: Default::default(),
            rate: 0.0,
        }
    }

    /// Retune the resonators (once a block).
    pub fn tune(&mut self, s: &GrainShape, rate: f32) {
        for (i, (l, r)) in self.bands.iter_mut().enumerate() {
            let t = (i as f32 + 0.5) / Self::BANDS as f32;
            let hz = s.lo * (s.hi / s.lo.max(1.0)).powf(t);
            l.bandpass(hz, s.q, rate);
            r.bandpass(hz * 1.03, s.q, rate);
        }
    }

    /// Start events for one block and run it, adding the result into `l`
    /// and `r`. `gain` is given to each event as it starts, so a crowd that
    /// is turned down lets the ones already sounding finish.
    pub fn run(&mut self, s: &GrainShape, sr: f32, gain: f32, l: &mut [f32], r: &mut [f32]) {
        if gain <= 0.0 && self.n == 0 {
            return;
        }
        for i in 0..l.len() {
            // A Poisson process: an event starts with probability `rate`
            // each sample (never more than one, which at these rates is the
            // same thing).
            if gain > 0.0 && self.n < Self::MAX && self.rng.unit() < self.rate {
                let tail = self.rng.range(s.decay[0], s.decay[1]);
                let amp = gain * (0.35 + 0.65 * self.rng.unit());
                let (pl, pr) = pan(s.centre + self.rng.bi() * s.width);
                let band = (self.rng.next() % Self::BANDS as u32) as u8;
                self.live[self.n] = (amp, (-1.0 / (tail * sr)).exp(), pl, pr, band, amp * 1e-3);
                self.n += 1;
            }
            let mut ins = [(0.0f32, 0.0f32); Self::BANDS];
            let mut k = 0;
            while k < self.n {
                let g = &mut self.live[k];
                let x = self.rng.bi() * g.0;
                ins[g.4 as usize].0 += x * g.2;
                ins[g.4 as usize].1 += x * g.3;
                g.0 *= g.1;
                if g.0 < g.5 {
                    self.n -= 1;
                    self.live[k] = self.live[self.n];
                } else {
                    k += 1;
                }
            }
            let (mut ol, mut or) = (0.0, 0.0);
            for (b, (il, ir)) in self.bands.iter_mut().zip(ins) {
                ol += b.0.run(il);
                or += b.1.run(ir);
            }
            l[i] += ol;
            r[i] += or;
        }
    }
}

/// Bubbles: van den Doel's model of liquid sound (2005). A bubble rings at
/// its Minnaert frequency, about 3.26 / radius Hz, for a few milliseconds,
/// its pitch rising as it goes; a stream is a crowd of them.
pub struct Bubbles {
    rng: Rng,
    /// phase, phase step, step multiplier per sample, amplitude, decay,
    /// left, right, and the amplitude it is let go at
    live: [[f32; 8]; Bubbles::MAX],
    n: usize,
    /// Expected bubbles per sample.
    pub rate: f32,
}

/// What sizes of bubble a body of water makes.
#[derive(Clone, Copy, Debug)]
pub struct BubbleShape {
    /// Radii, millimetres: the smallest and largest. Small ones are commoner.
    pub r: [f32; 2],
    /// How far the pitch rises over a bubble's life, as a share of where it
    /// starts.
    pub rise: f32,
    /// As `GrainShape`: where the bubbles are centred and how widely spread.
    pub centre: f32,
    pub width: f32,
}

impl Bubbles {
    pub const MAX: usize = 128;

    pub fn new(seed: u32) -> Bubbles {
        Bubbles { rng: Rng::new(seed), live: [[0.0; 8]; Bubbles::MAX], n: 0, rate: 0.0 }
    }

    pub fn run(&mut self, s: &BubbleShape, sr: f32, gain: f32, l: &mut [f32], r: &mut [f32]) {
        if gain <= 0.0 && self.n == 0 {
            return;
        }
        for i in 0..l.len() {
            if gain > 0.0 && self.n < Self::MAX && self.rng.unit() < self.rate {
                // Radius drawn toward the small end (a square of a uniform).
                let u = self.rng.unit();
                let mm = s.r[0] + (s.r[1] - s.r[0]) * u * u;
                let hz = 3.26 / (mm * 1e-3);
                // Bigger bubbles ring longer and louder.
                let life = 0.004 + 0.006 * mm;
                let amp = gain * (0.25 + 0.75 * (mm / s.r[1].max(0.1))).min(1.0) * (0.5 + 0.5 * self.rng.unit());
                let step = TAU * hz.min(sr * 0.45) / sr;
                let mult = (1.0 + s.rise).powf(1.0 / (life * 3.0 * sr));
                let (pl, pr) = pan(s.centre + self.rng.bi() * s.width);
                self.live[self.n] = [0.0, step, mult, amp, (-1.0 / (life * sr)).exp(), pl, pr, amp * 1e-3];
                self.n += 1;
            }
            let (mut ol, mut or) = (0.0, 0.0);
            let mut k = 0;
            while k < self.n {
                let b = &mut self.live[k];
                let x = b[0].sin() * b[3];
                ol += x * b[5];
                or += x * b[6];
                b[0] += b[1];
                if b[0] > TAU {
                    b[0] -= TAU;
                }
                b[1] *= b[2];
                b[3] *= b[4];
                if b[3] < b[7] {
                    self.n -= 1;
                    self.live[k] = self.live[self.n];
                } else {
                    k += 1;
                }
            }
            l[i] += ol;
            r[i] += or;
        }
    }
}

/// The room: one small feedback delay network, a send every voice may use.
/// Four delay lines mixed through a Hadamard matrix with a lowpass in each
/// loop; `size` stretches the lines (a wood is short, a canyon long) and
/// lengthens the tail with them.
pub struct Room {
    lines: [Vec<f32>; 4],
    w: usize,
    damp: [OnePole; 4],
    len: [f32; 4],
    fb: f32,
}

/// Base lengths of the four lines, seconds; mutually prime in samples at
/// any common rate, so the echoes never line up.
const ROOM_BASE: [f32; 4] = [0.0297, 0.0371, 0.0411, 0.0437];
/// The longest a line stretches to, as a multiple of its base.
const ROOM_STRETCH: f32 = 2.6;

impl Room {
    pub fn new(sr: f32) -> Room {
        let cap = (ROOM_BASE[3] * ROOM_STRETCH * sr) as usize + 4;
        Room {
            lines: [vec![0.0; cap], vec![0.0; cap], vec![0.0; cap], vec![0.0; cap]],
            w: 0,
            damp: Default::default(),
            len: [1.0; 4],
            fb: 0.0,
        }
    }

    /// `size` 0 (a wood: short, dark) to 1 (a canyon or a cave: long).
    #[allow(clippy::needless_range_loop)] // four parallel arrays, one line each
    pub fn shape(&mut self, size: f32, sr: f32) {
        let s = size.clamp(0.0, 1.0);
        for k in 0..4 {
            self.len[k] = ROOM_BASE[k] * (0.6 + (ROOM_STRETCH - 0.6) * s) * sr;
            self.damp[k].set(2600.0 + 2400.0 * s, sr);
        }
        // Feedback for a tail of roughly 0.4 s in a wood to 2.5 s in a cave.
        self.fb = 0.62 + 0.26 * s;
    }

    #[inline]
    fn read(&self, k: usize) -> f32 {
        let line = &self.lines[k];
        let cap = line.len();
        let d = self.len[k].clamp(1.0, (cap - 2) as f32);
        let pos = self.w as f32 + cap as f32 - d;
        let i = pos as usize;
        let f = pos - i as f32;
        let a = line[i % cap];
        let b = line[(i + 1) % cap];
        a + (b - a) * f
    }

    /// One sample in, a stereo pair out.
    #[inline]
    #[allow(clippy::needless_range_loop)] // four parallel arrays, one line each
    pub fn run(&mut self, x: f32) -> (f32, f32) {
        let o = [self.read(0), self.read(1), self.read(2), self.read(3)];
        // Hadamard, scaled to keep the energy.
        let h = [
            (o[0] + o[1] + o[2] + o[3]) * 0.5,
            (o[0] - o[1] + o[2] - o[3]) * 0.5,
            (o[0] + o[1] - o[2] - o[3]) * 0.5,
            (o[0] - o[1] - o[2] + o[3]) * 0.5,
        ];
        let cap = self.lines[0].len();
        for k in 0..4 {
            let v = self.damp[k].lp(h[k] * self.fb) + x * 0.5;
            self.lines[k][self.w] = v;
        }
        self.w = (self.w + 1) % cap;
        (o[0] + o[2], o[1] + o[3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len() as f32).sqrt()
    }

    #[test]
    fn noise_is_bounded_and_not_silent() {
        let mut p = Pink::new(1);
        let mut b = Brown::new(2);
        let pv: Vec<f32> = (0..48_000).map(|_| p.next()).collect();
        let bv: Vec<f32> = (0..48_000).map(|_| b.next()).collect();
        for v in [&pv, &bv] {
            assert!(v.iter().all(|x| x.is_finite() && x.abs() < 2.0));
            let r = rms(v);
            assert!(r > 0.05 && r < 0.8, "rms {r}");
        }
    }

    /// A bandpass passes its centre and turns away a tone two octaves off.
    #[test]
    fn a_bandpass_passes_its_centre() {
        let sr = 48_000.0;
        let tone = |hz: f32| {
            let mut f = Biquad::default();
            f.bandpass(1000.0, 2.0, sr);
            let out: Vec<f32> = (0..48_000).map(|i| f.run((TAU * hz * i as f32 / sr).sin())).collect();
            rms(&out[4800..])
        };
        let at = tone(1000.0);
        let off = tone(4000.0);
        assert!((at - 0.707).abs() < 0.05, "centre {at}");
        assert!(off < at * 0.3, "off-centre {off}");
    }

    #[test]
    fn a_glide_arrives_without_overshoot() {
        let mut g = Glide::new(0.0, 0.5, 750.0);
        g.to = 1.0;
        let mut last = 0.0;
        for _ in 0..3000 {
            let v = g.step();
            assert!(v >= last && v <= 1.0);
            last = v;
        }
        assert!(last > 0.999);
    }

    /// The room rings on after its input stops, then dies away.
    #[test]
    fn the_room_has_a_tail_that_ends() {
        let sr = 48_000.0;
        let mut r = Room::new(sr);
        r.shape(1.0, sr);
        let mut out = vec![];
        for i in 0..(sr as usize * 6) {
            let x = if i < 480 { 1.0 } else { 0.0 };
            let (a, b) = r.run(x);
            out.push(a + b);
        }
        let early = rms(&out[4800..24_000]);
        let late = rms(&out[out.len() - 24_000..]);
        assert!(early > 0.005, "no tail: {early}");
        assert!(late < early * 0.01, "it never dies: {late} vs {early}");
        assert!(out.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn bubbles_ring_and_stop() {
        let sr = 48_000.0;
        let mut b = Bubbles::new(3);
        let s = BubbleShape { r: [0.5, 5.0], rise: 0.2, centre: 0.0, width: 0.5 };
        b.rate = 200.0 / sr;
        let (mut l, mut r) = (vec![0.0; 48_000], vec![0.0; 48_000]);
        b.run(&s, sr, 1.0, &mut l, &mut r);
        assert!(rms(&l) > 0.01);
        // Stop starting them: the crowd empties within a fraction of a second.
        let (mut l2, mut r2) = (vec![0.0; 24_000], vec![0.0; 24_000]);
        b.run(&s, sr, 0.0, &mut l2, &mut r2);
        assert_eq!(b.n, 0);
    }
}
