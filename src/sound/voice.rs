//! The ambient voices. Each is a few filters over noise, most with a crowd of
//! short events on top, and each reads the moment's numbers (`Now`) and adds
//! itself into a block of left, right and reverb-send samples.
//!
//! Every voice is written to come out near the same loudness at full
//! strength (`LEVEL`), so the numbers in `Heard` say how loud each one is
//! relative to the others and nothing else does.

use super::dsp::{pan, Biquad, BubbleShape, Bubbles, Brown, GrainShape, Grains, OnePole, Pink, Rng};
use super::Ground;

/// The smoothed numbers a block is made from: `Heard` after its glides,
/// plus the moment's gust and where each emitter sits in the stereo field.
#[derive(Clone, Copy, Default, Debug)]
pub struct Now {
    /// The wind this instant, gusts included: 0 still to about 1.6.
    pub blow: f32,
    pub wind: f32,
    pub whistle: f32,
    pub whine: f32,
    pub leaves: f32,
    pub grass: f32,
    pub rain: f32,
    pub shelter: f32,
    pub ground: Ground,
    pub drip: f32,
    pub water: f32,
    pub water_pan: (f32, f32),
    pub flow: f32,
    pub falls: f32,
    pub falls_pan: (f32, f32),
    pub falls_size: f32,
    pub spray: f32,
    pub surf: f32,
    pub surf_pan: (f32, f32),
    pub under: f32,
}

/// One block's worth of output, filled by every voice in turn.
pub struct Mix<'a> {
    pub l: &'a mut [f32],
    pub r: &'a mut [f32],
    pub sr: f32,
}

/// A pair of independent noise streams through a pair of filters: the way
/// every bed in here is made, so no bed sits in the middle of the head.
struct Bed<N> {
    n: [N; 2],
    f: [Biquad; 2],
}

/// The air moving: a low body, a broad band that the gusts push up and
/// down, and the whistle of an edge.
pub struct Wind {
    body: [(Pink, OnePole); 2],
    air: Bed<Pink>,
    whistle: [(Rng, Biquad, Biquad); 2],
}

impl Wind {
    pub fn new(seed: u32) -> Wind {
        Wind {
            body: [(Pink::new(seed), OnePole::default()), (Pink::new(seed + 1), OnePole::default())],
            air: Bed { n: [Pink::new(seed + 2), Pink::new(seed + 3)], f: Default::default() },
            whistle: [(Rng::new(seed + 4), Biquad::default(), Biquad::default()), (Rng::new(seed + 5), Biquad::default(), Biquad::default())],
        }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        if n.wind <= 0.0 && n.whistle <= 0.0 {
            return;
        }
        let s = n.blow;
        let body = n.wind * 1.1 * s.powf(0.8);
        let air = n.wind * 0.9 * s.powf(1.6);
        // A whistle is a narrow band, so it needs a lot more gain than a
        // broad one to be heard at all.
        let whistle = n.whistle * 7.5 * (s * s).min(2.5);
        for (k, (p, lp)) in self.body.iter_mut().enumerate() {
            lp.set(110.0 + 260.0 * s, m.sr);
            self.air.f[k].bandpass(420.0 + 1500.0 * s, 0.9, m.sr);
            let w = &mut self.whistle[k];
            let hz = 650.0 + 1500.0 * n.whine + 500.0 * s + 40.0 * k as f32;
            w.1.bandpass(hz, 16.0, m.sr);
            w.2.bandpass(hz * 1.5, 12.0, m.sr);
            let out = if k == 0 { &mut *m.l } else { &mut *m.r };
            for o in out.iter_mut() {
                let b = lp.lp(p.next()) * 2.2;
                let a = self.air.f[k].run(self.air.n[k].next());
                let x = w.0.bi();
                let wh = w.1.run(x) + 0.4 * w.2.run(x);
                *o += b * body + a * air + wh * whistle;
            }
        }
    }
}

/// Leaves in a crown, or a sward of grass: a hiss that rides the gusts,
/// for leaves with a crowd of individual slaps on top. One struct, two
/// settings.
///
/// Leaves are a few slaps you can pick out, each rung by the leaf. Grass is
/// thousands of blades brushing, and any crowd of sharp events over it,
/// however dense, sounds like rain: every event starts at once, and that is
/// what a drop does. So grass has no events at all. It is the hiss alone,
/// moved two ways: a sway, the sward leaning and coming back over a second
/// or so, brighter as it leans, and a flutter, the blades shivering, a few
/// dozen times a second. Both glide, so nothing in it ever starts sharply.
pub struct Rustle {
    bed: Bed<Rng>,
    grains: Grains,
    kind: Rustling,
    /// Each ear's sway and flutter.
    sway: [Wander; 2],
    flutter: [Wander; 2],
    /// A lowpass over the bed, where the shape asks for one.
    top: [OnePole; 2],
    rng: Rng,
}

#[derive(Clone, Copy)]
pub enum Rustling {
    Leaves,
    Grass,
}

/// How a rustle is made.
struct RustleShape {
    /// Bed centre, how far a gale lifts it, and its Q.
    hz: f32,
    lift: f32,
    q: f32,
    /// How much bed is left with no wind at all, against a gale's 0.6.
    floor: f32,
    /// Slaps a second at rest and in a gale, their sound and how loud each
    /// is.
    rates: [f32; 2],
    grain: GrainShape,
    crowd: f32,
    /// How deep the sway and the flutter go, 0 to 1.
    sway: f32,
    flutter: f32,
    /// The lowpass over the bed in Hz, or 0 for none.
    top: f32,
    /// How much to lift it all to sit level with the other voices.
    loud: f32,
}

impl Rustling {
    fn shape(self) -> RustleShape {
        match self {
            Rustling::Leaves => RustleShape {
                hz: 2600.0,
                lift: 600.0,
                q: 0.8,
                floor: 0.05,
                rates: [3.0, 90.0],
                grain: GrainShape { lo: 1600.0, hi: 6000.0, q: 3.0, decay: [0.004, 0.013], centre: 0.0, width: 0.9 },
                crowd: 2.2,
                sway: 0.0,
                flutter: 0.0,
                top: 0.0,
                loud: 4.0,
            },
            Rustling::Grass => RustleShape {
                hz: 1500.0,
                lift: 700.0,
                q: 0.5,
                floor: 0.15,
                rates: [0.0, 0.0],
                grain: GrainShape { lo: 1000.0, hi: 1000.0, q: 1.0, decay: [0.01, 0.01], centre: 0.0, width: 1.0 },
                crowd: 0.0,
                sway: 0.8,
                flutter: 0.4,
                top: 4500.0,
                loud: 5.2,
            },
        }
    }
}

/// A value wandering between 0 and 1: it picks somewhere new every so
/// often and glides there.
#[derive(Clone, Copy)]
struct Wander {
    to: f32,
    at: f32,
    left: u32,
}

impl Default for Wander {
    fn default() -> Wander {
        Wander { to: 0.5, at: 0.5, left: 0 }
    }
}

impl Wander {
    /// One sample on: `every` is the seconds between picks, at least and
    /// at most, and `glide` the share of the way covered each sample.
    #[inline]
    fn next(&mut self, rng: &mut Rng, every: [f32; 2], glide: f32, sr: f32) -> f32 {
        if self.left == 0 {
            self.to = rng.unit();
            self.left = (rng.range(every[0], every[1]) * sr) as u32 + 1;
        }
        self.left -= 1;
        self.at += glide * (self.to - self.at);
        self.at
    }
}

/// Seconds between the sway picking a new lean, and the flutter a new
/// shiver, at least and at most.
const SWAY_S: [f32; 2] = [0.4, 1.6];
const FLUTTER_S: [f32; 2] = [0.015, 0.05];

impl Rustle {
    pub fn new(seed: u32, kind: Rustling) -> Rustle {
        Rustle {
            bed: Bed { n: [Rng::new(seed), Rng::new(seed + 1)], f: Default::default() },
            grains: Grains::new(seed + 2),
            kind,
            sway: Default::default(),
            flutter: Default::default(),
            top: Default::default(),
            rng: Rng::new(seed + 3),
        }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        let gain = match self.kind {
            Rustling::Leaves => n.leaves,
            Rustling::Grass => n.grass,
        };
        let sh = self.kind.shape();
        // A plant only rustles when something moves it.
        let s = n.blow.min(1.6);
        let bed = gain * sh.loud * (sh.floor + 0.6 * s * s);
        // The sway takes a quarter of a second to lean and swings harder
        // the harder it blows; the flutter takes a hundredth.
        let slow = 1.0 - (-1.0 / (0.25 * m.sr)).exp();
        let fast = 1.0 - (-1.0 / (0.01 * m.sr)).exp();
        let depth = sh.sway * (0.5 + 0.5 * s.min(1.0));
        for k in 0..2 {
            // Leaning hard, the sward is brighter as well as louder.
            let l = lean(depth, self.sway[k].at);
            self.bed.f[k].bandpass((sh.hz + sh.lift * s) * (0.8 + 0.2 * l), sh.q, m.sr);
            self.top[k].set(sh.top, m.sr);
            let out = if k == 0 { &mut *m.l } else { &mut *m.r };
            for o in out.iter_mut() {
                let w = self.sway[k].next(&mut self.rng, SWAY_S, slow, m.sr);
                let f = if sh.flutter > 0.0 { 1.0 + sh.flutter * (2.0 * self.flutter[k].next(&mut self.rng, FLUTTER_S, fast, m.sr) - 1.0) } else { 1.0 };
                let x = self.bed.f[k].run(self.bed.n[k].bi());
                let x = if sh.top > 0.0 { self.top[k].lp(x) } else { x };
                *o += x * bed * lean(depth, w) * f;
            }
        }
        if sh.crowd > 0.0 {
            self.grains.rate = if gain > 0.0 { rates_at(sh.rates, s) / m.sr } else { 0.0 };
            self.grains.tune(&sh.grain, m.sr);
            self.grains.run(&sh.grain, m.sr, gain * sh.loud * sh.crowd, m.l, m.r);
        }
    }
}

/// How hard the sward leans, from a sway `w` of 0 to 1: at full `depth`
/// 0.4 to 2.8, averaging 1 over a sway that wanders evenly, so swaying
/// moves the rustle about without making it louder. The swish.
fn lean(depth: f32, w: f32) -> f32 {
    1.0 + depth * (2.4 * w * w * w - 0.6)
}

/// Events a second between rest and a gale, for a blow `s` of 0 to 1.6.
fn rates_at(r: [f32; 2], s: f32) -> f32 {
    r[0] + (r[1] - r[0]) * (s * s / 2.5)
}

/// What a raindrop sounds like landing on each kind of ground: the crowd
/// of drops rings a resonator, and the resonator is the ground's.
fn drop_shape(g: Ground) -> GrainShape {
    let (lo, hi, q, decay) = match g {
        Ground::Leaves => (700.0, 2600.0, 3.0, [0.006, 0.02]),
        Ground::Water => (1200.0, 5000.0, 9.0, [0.01, 0.035]),
        Ground::Stone => (2500.0, 8000.0, 5.0, [0.003, 0.01]),
        Ground::Grass => (1500.0, 6000.0, 2.0, [0.003, 0.008]),
        Ground::Soft => (400.0, 1500.0, 1.5, [0.002, 0.006]),
    };
    GrainShape { lo, hi, q, decay, centre: 0.0, width: 1.0 }
}

/// Rain in three layers: the far hiss of it on the whole land, the near
/// drops on whatever is underfoot, and drips off the leaves overhead, which
/// go on after the rain has stopped.
pub struct Rain {
    hiss: [(Pink, Biquad); 2],
    drops: Grains,
    drips: Grains,
    ground: Option<Ground>,
}

const DRIP: GrainShape = GrainShape { lo: 450.0, hi: 2200.0, q: 12.0, decay: [0.02, 0.06], centre: 0.0, width: 0.8 };

impl Rain {
    pub fn new(seed: u32) -> Rain {
        Rain {
            hiss: [(Pink::new(seed), Biquad::default()), (Pink::new(seed + 1), Biquad::default())],
            drops: Grains::new(seed + 2),
            drips: Grains::new(seed + 3),
            ground: None,
        }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        let open = 1.0 - n.shelter;
        if n.rain > 0.0 {
            // Under a roof the hiss loses its top: rain on the roof, not on you.
            let hiss = n.rain.powf(0.8) * (0.45 + 0.55 * open) * 1.6;
            for (k, (p, f)) in self.hiss.iter_mut().enumerate() {
                f.lowpass(700.0 + 2400.0 * open, 0.6, m.sr);
                let out = if k == 0 { &mut *m.l } else { &mut *m.r };
                for o in out.iter_mut() {
                    *o += f.run(p.next()) * hiss;
                }
            }
        }
        let shape = drop_shape(n.ground);
        if self.ground != Some(n.ground) {
            self.drops.tune(&shape, m.sr);
            self.drips.tune(&DRIP, m.sr);
            self.ground = Some(n.ground);
        }
        let near = n.rain * (1.0 - 0.85 * n.shelter);
        self.drops.rate = near * (30.0 + 220.0 * n.rain) / m.sr;
        self.drops.run(&shape, m.sr, near.sqrt() * 2.2, m.l, m.r);
        self.drips.rate = n.drip * (0.5 + 3.0 * n.drip) / m.sr;
        self.drips.run(&DRIP, m.sr, 24.0, m.l, m.r);
    }
}

/// A bed of noise panned toward an emitter, still decorrelated between the
/// ears. `p` is (centre, width) as `Now` carries it.
fn panned_bed<N>(b: &mut Bed<N>, gain: f32, p: (f32, f32), m: &mut Mix, mut next: impl FnMut(&mut N) -> f32) {
    if gain <= 0.0 {
        return;
    }
    let (pl, pr) = pan(p.0);
    // A source heard all round is as loud in both ears whatever its centre.
    let gl = pl + (std::f32::consts::FRAC_1_SQRT_2 - pl) * p.1;
    let gr = pr + (std::f32::consts::FRAC_1_SQRT_2 - pr) * p.1;
    for (k, g) in [gl, gr].into_iter().enumerate() {
        let out = if k == 0 { &mut *m.l } else { &mut *m.r };
        for o in out.iter_mut() {
            *o += b.f[k].run(next(&mut b.n[k])) * gain * g * std::f32::consts::SQRT_2;
        }
    }
}

/// Running water: bubbles over a bed of turbulence. Fast shallow water is
/// many small bubbles and a bright bed; slow water, a few, quieter, over a
/// low murmur. Slow water at full strength was once a hundred big bubbles a
/// second, and a slow creek sounded like a pot on the boil.
pub struct Brook {
    bubbles: Bubbles,
    churn: Bed<Pink>,
}

impl Brook {
    pub fn new(seed: u32) -> Brook {
        Brook { bubbles: Bubbles::new(seed), churn: Bed { n: [Pink::new(seed + 1), Pink::new(seed + 2)], f: Default::default() } }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        let f = n.flow.clamp(0.0, 1.0);
        let shape = BubbleShape { r: [0.6, 2.5 + 2.0 * (1.0 - f)], rise: 0.12 + 0.25 * f, centre: n.water_pan.0, width: 0.15 + 0.85 * n.water_pan.1 };
        self.bubbles.rate = if n.water > 0.0 { (10.0 + 200.0 * f + 400.0 * f * f) / m.sr } else { 0.0 };
        self.bubbles.run(&shape, m.sr, n.water * (1.05 - 0.55 * f) * 1.9, m.l, m.r);
        for b in &mut self.churn.f {
            b.bandpass(420.0 + 700.0 * f, 0.6, m.sr);
        }
        panned_bed(&mut self.churn, n.water * (0.25 + 0.75 * f) * 1.9, n.water_pan, m, |p| p.next());
    }
}

/// A waterfall: a dense crowd of bubbles, the roar of the plunge, a rumble
/// that comes with the height of the drop, and spray close to its foot.
pub struct Falls {
    bubbles: Bubbles,
    roar: Bed<Pink>,
    rumble: Bed<Brown>,
    spray: Bed<Rng>,
}

impl Falls {
    pub fn new(seed: u32) -> Falls {
        Falls {
            bubbles: Bubbles::new(seed),
            roar: Bed { n: [Pink::new(seed + 1), Pink::new(seed + 2)], f: Default::default() },
            rumble: Bed { n: [Brown::new(seed + 3), Brown::new(seed + 4)], f: Default::default() },
            spray: Bed { n: [Rng::new(seed + 5), Rng::new(seed + 6)], f: Default::default() },
        }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        if n.falls <= 0.0 {
            self.bubbles.rate = 0.0;
            let shape = BubbleShape { r: [0.5, 6.0], rise: 0.3, centre: 0.0, width: 1.0 };
            self.bubbles.run(&shape, m.sr, 0.0, m.l, m.r);
            return;
        }
        let size = n.falls_size.clamp(0.0, 1.5);
        let shape = BubbleShape { r: [0.5, 6.0], rise: 0.3, centre: n.falls_pan.0, width: 0.2 + 0.8 * n.falls_pan.1 };
        self.bubbles.rate = (600.0 + 900.0 * size.min(1.0)) / m.sr;
        self.bubbles.run(&shape, m.sr, n.falls * 0.5, m.l, m.r);
        for k in 0..2 {
            self.roar.f[k].bandpass(320.0 - 80.0 * size.min(1.0), 0.5, m.sr);
            self.rumble.f[k].lowpass(70.0 + 50.0 * size.min(1.0), 0.7, m.sr);
            // The spray is the high end alone.
            self.spray.f[k].highpass(2500.0, 0.7, m.sr);
        }
        panned_bed(&mut self.roar, n.falls * 0.9, n.falls_pan, m, |p| p.next());
        panned_bed(&mut self.rumble, n.falls * size * 1.6, (n.falls_pan.0 * 0.5, 1.0), m, |p| p.next());
        panned_bed(&mut self.spray, n.spray * 0.5, n.falls_pan, m, |r| r.bi());
    }
}

/// Waves on a shore: three trains of swells at their own slowly wandering
/// periods, each breaking into a wash of foam just after it peaks. Filtered
/// noise rather than anything physical, which is what scored best in Tez,
/// Selfridge and Reiss's listening test (AES 2022).
pub struct Surf {
    trains: [Train; 3],
    rng: Rng,
}

struct Train {
    t: f32,
    period: f32,
    foam: f32,
    foam_to: f32,
    body: [(Brown, Biquad); 2],
    wash: [(Rng, Biquad); 2],
    offset: f32,
}

impl Surf {
    pub fn new(seed: u32) -> Surf {
        let mut rng = Rng::new(seed);
        let trains = std::array::from_fn(|k| {
            let s = seed + 10 * (k as u32 + 1);
            Train {
                t: rng.range(0.0, 12.0),
                period: rng.range(8.0, 14.0),
                foam: 0.0,
                foam_to: 0.0,
                body: [(Brown::new(s), Biquad::default()), (Brown::new(s + 1), Biquad::default())],
                wash: [(Rng::new(s + 2), Biquad::default()), (Rng::new(s + 3), Biquad::default())],
                offset: k as f32 - 1.0,
            }
        });
        Surf { trains, rng }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        let dt = m.l.len() as f32 / m.sr;
        let gain = n.surf;
        for tr in &mut self.trains {
            // The clock runs whether or not anyone is listening, so the sea
            // is mid-swell when you come over the dune, not starting one.
            let before = tr.t / tr.period;
            tr.t += dt;
            if tr.t >= tr.period {
                tr.t -= tr.period;
                tr.period = (tr.period + self.rng.range(-1.5, 1.5)).clamp(8.0, 14.0);
            }
            let x = tr.t / tr.period;
            if before < 0.55 && x >= 0.55 {
                tr.foam_to = 0.6 + 0.4 * self.rng.unit();
            }
            // Foam: 0.6 s up, then a 2.5 s fall.
            if tr.foam_to > tr.foam {
                tr.foam = (tr.foam + dt / 0.6).min(tr.foam_to);
                if tr.foam >= tr.foam_to {
                    tr.foam_to = 0.0;
                }
            } else {
                tr.foam *= (-dt / 2.5).exp();
            }
            if gain <= 0.0 {
                continue;
            }
            let swell = (std::f32::consts::PI * x).sin().powi(2);
            let c = (n.surf_pan.0 + tr.offset * 0.35 * n.surf_pan.1).clamp(-1.0, 1.0);
            let (pl, pr) = pan(c * (1.0 - 0.5 * n.surf_pan.1));
            for k in 0..2 {
                let (b, bf) = &mut tr.body[k];
                bf.lowpass(220.0 + 520.0 * swell, 0.7, m.sr);
                let (w, wf) = &mut tr.wash[k];
                wf.bandpass(1500.0 + 900.0 * tr.foam, 0.6, m.sr);
                let g = if k == 0 { pl } else { pr } * std::f32::consts::SQRT_2;
                let (body, wash) = (gain * g * (0.2 + 0.8 * swell) * 2.8, gain * g * tr.foam * 2.1);
                let out = if k == 0 { &mut *m.l } else { &mut *m.r };
                for o in out.iter_mut() {
                    *o += bf.run(b.next()) * body + wf.run(w.bi()) * wash;
                }
            }
        }
    }
}

/// The ear under the surface: a slow rumble and the odd bubble. Everything
/// else is muffled by the engine on its way out.
pub struct Under {
    rumble: Bed<Brown>,
    bubbles: Bubbles,
}

impl Under {
    pub fn new(seed: u32) -> Under {
        Under { rumble: Bed { n: [Brown::new(seed), Brown::new(seed + 1)], f: Default::default() }, bubbles: Bubbles::new(seed + 2) }
    }

    pub fn run(&mut self, n: &Now, m: &mut Mix) {
        for f in &mut self.rumble.f {
            f.lowpass(160.0, 0.7, m.sr);
        }
        panned_bed(&mut self.rumble, n.under * 3.0, (0.0, 1.0), m, |p| p.next());
        let shape = BubbleShape { r: [1.5, 6.0], rise: 0.4, centre: 0.0, width: 0.8 };
        self.bubbles.rate = if n.under > 0.0 { 6.0 / m.sr } else { 0.0 };
        self.bubbles.run(&shape, m.sr, n.under * 1.0, m.l, m.r);
    }
}
