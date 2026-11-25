//! Animal calls, on their own bus (Wildlife).
//!
//! A call is a `Pattern`: up to `SYLS` syllables, each with its own start
//! time, length, pitch contour and source, so they can follow one another
//! (a crow's caws), overlap (an elk's whistle over its own growl, which is
//! the two voices a bugle really has) or run on with the pitch unbroken
//! (`tie`: a howl's rise into its fall). The four sources are what animal
//! voices are made of:
//!
//! - `Tone`: a whistle, near a pure sine with a little of its second and
//!   third harmonics. Birdsong, owls, a coyote's howl, the top of a bugle.
//! - `Voiced`: a pulse train (a band-limited saw, darkened) through three
//!   formant filters that glide: the source-filter model of a mammal's
//!   larynx and vocal tract. Moos, whinnies, trumpets. Each period wavers
//!   a little in length and loudness (jitter and shimmer), which is most of
//!   what keeps it from sounding like a machine.
//! - `Rough`: voiced, with every other period weaker and longer (a
//!   subharmonic, the vocal folds out of step) and a puff of noise on each
//!   pulse. Roars, growls, caws.
//! - `Hiss`: noise through the formants. Snorts, blows, hisses.
//!
//! A call's numbers come from its kind (`Cry`), the caller's size (bigger
//! is lower and a little slower) and a seed: the same seed is the same
//! call, so one bird keeps its own song and its neighbour sings another.
//! Nothing here allocates once the pool is made.

use super::dsp::{pan, Biquad, OnePole, Rng};
use std::f32::consts::{PI, TAU};

/// What kind of call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cry {
    /// A songbird's song: a few phrases of whistles, trills and buzzes.
    Song,
    /// A crow's caws.
    Caw,
    /// A great horned owl's hoots.
    Hoot,
    /// A coyote's (or a wolf's) howl.
    Howl,
    /// A coyote's yips.
    Yip,
    /// A bull elk in the rut.
    Bugle,
    Moo,
    /// A horse's whinny.
    Neigh,
    /// An elephant's trumpet, and its rumble (mostly under hearing).
    Trumpet,
    Rumble,
    /// A lion's roaring bout: moans, full roars, then grunts.
    Roar,
    /// A field cricket.
    Chirp,
    /// A deer's alarm blow.
    Snort,
    /// A fox's bark, a sea lion's, a prairie dog's.
    Bark,
    /// A sheep's baa, quavering.
    Bleat,
    /// A pig's or a wildebeest's grunts, a hippo's at size.
    Grunt,
    /// A goose's honks.
    Honk,
    /// A duck's run of quacks.
    Quack,
    /// A hawk's scream, or a falcon's cackle.
    Screech,
    /// A gull's long call.
    Gull,
    /// Shorebirds' and quail's whistled notes; a marmot's alarm.
    Pipe,
    /// A squirrel's or a kingfisher's harsh rattle of notes.
    Chatter,
    /// A woodpecker's bill on wood.
    Drum,
    /// A rattlesnake's tail.
    Rattle,
    /// A frog's rib-bit; a bullfrog's at size.
    Croak,
    /// A toad's long trill, a nightjar's churr.
    Trill,
    /// A snake's, a crocodile's or a goose's hiss.
    Hiss,
    /// An alligator's bellow, a walrus's.
    Bellow,
    /// A bee's wings.
    Buzz,
    /// A bear's or a big cat's growl, or a jaguar sawing.
    Growl,
    /// A cougar's or a vixen's scream.
    Scream,
    /// A hyena's rising whoops.
    Whoop,
    /// A donkey's hee-haw.
    Bray,
    /// A whale's spout.
    Blow,
    /// Long sliding tones: a humpback's song.
    Moan,
    /// A zombie.
    Groan,
    /// Wings clattering as a bird gets up.
    Flap,
    /// A fish or a ray leaping and falling back.
    Splash,
    /// Horns or skulls meeting; antlers clattering.
    Clash,
    /// A rabbit stamping a hind foot.
    Thump,
}

pub const CRIES: [Cry; 40] = [
    Cry::Song,
    Cry::Caw,
    Cry::Hoot,
    Cry::Howl,
    Cry::Yip,
    Cry::Bugle,
    Cry::Moo,
    Cry::Neigh,
    Cry::Trumpet,
    Cry::Rumble,
    Cry::Roar,
    Cry::Chirp,
    Cry::Snort,
    Cry::Bark,
    Cry::Bleat,
    Cry::Grunt,
    Cry::Honk,
    Cry::Quack,
    Cry::Screech,
    Cry::Gull,
    Cry::Pipe,
    Cry::Chatter,
    Cry::Drum,
    Cry::Rattle,
    Cry::Croak,
    Cry::Trill,
    Cry::Hiss,
    Cry::Bellow,
    Cry::Buzz,
    Cry::Growl,
    Cry::Scream,
    Cry::Whoop,
    Cry::Bray,
    Cry::Blow,
    Cry::Moan,
    Cry::Groan,
    Cry::Flap,
    Cry::Splash,
    Cry::Clash,
    Cry::Thump,
];

impl Cry {
    pub fn name(self) -> &'static str {
        match self {
            Cry::Song => "song",
            Cry::Caw => "caw",
            Cry::Hoot => "hoot",
            Cry::Howl => "howl",
            Cry::Yip => "yip",
            Cry::Bugle => "bugle",
            Cry::Moo => "moo",
            Cry::Neigh => "neigh",
            Cry::Trumpet => "trumpet",
            Cry::Rumble => "rumble",
            Cry::Roar => "roar",
            Cry::Chirp => "chirp",
            Cry::Snort => "snort",
            Cry::Bark => "bark",
            Cry::Bleat => "bleat",
            Cry::Grunt => "grunt",
            Cry::Honk => "honk",
            Cry::Quack => "quack",
            Cry::Screech => "screech",
            Cry::Gull => "gull",
            Cry::Pipe => "pipe",
            Cry::Chatter => "chatter",
            Cry::Drum => "drum",
            Cry::Rattle => "rattle",
            Cry::Croak => "croak",
            Cry::Trill => "trill",
            Cry::Hiss => "hiss",
            Cry::Bellow => "bellow",
            Cry::Buzz => "buzz",
            Cry::Growl => "growl",
            Cry::Scream => "scream",
            Cry::Whoop => "whoop",
            Cry::Bray => "bray",
            Cry::Blow => "blow",
            Cry::Moan => "moan",
            Cry::Groan => "groan",
            Cry::Flap => "flap",
            Cry::Splash => "splash",
            Cry::Clash => "clash",
            Cry::Thump => "thump",
        }
    }

    pub fn parse(s: &str) -> Option<Cry> {
        CRIES.iter().copied().find(|c| c.name() == s)
    }

    /// Roughly how loud the real thing is at 1 m, dB SPL, from field
    /// measurements: a songbird in the 80s, a lion's roar near 114.
    pub fn spl(self) -> f32 {
        match self {
            Cry::Song => 85.0,
            Cry::Caw => 90.0,
            Cry::Hoot => 85.0,
            Cry::Howl => 105.0,
            Cry::Yip => 95.0,
            Cry::Bugle => 108.0,
            Cry::Moo => 100.0,
            Cry::Neigh => 100.0,
            Cry::Trumpet => 112.0,
            Cry::Rumble => 105.0,
            Cry::Roar => 114.0,
            Cry::Chirp => 82.0,
            Cry::Snort => 90.0,
            Cry::Bark => 95.0,
            Cry::Bleat => 85.0,
            Cry::Grunt => 85.0,
            Cry::Honk => 95.0,
            Cry::Quack => 85.0,
            Cry::Screech => 95.0,
            Cry::Gull => 95.0,
            Cry::Pipe => 88.0,
            Cry::Chatter => 80.0,
            Cry::Drum => 95.0,
            Cry::Rattle => 70.0,
            Cry::Croak => 90.0,
            Cry::Trill => 85.0,
            Cry::Hiss => 70.0,
            Cry::Bellow => 105.0,
            Cry::Buzz => 55.0,
            Cry::Growl => 85.0,
            Cry::Scream => 100.0,
            Cry::Whoop => 100.0,
            Cry::Bray => 105.0,
            Cry::Blow => 100.0,
            Cry::Moan => 85.0,
            Cry::Groan => 80.0,
            Cry::Flap => 70.0,
            Cry::Splash => 85.0,
            Cry::Clash => 100.0,
            Cry::Thump => 75.0,
        }
    }

    /// A trim per kind so every pattern peaks near the same level dry
    /// (`tests::every_cry_peaks_alike`); loudness proper is `spl`.
    fn trim(self) -> f32 {
        match self {
            Cry::Song => 0.64,
            Cry::Caw => 0.89,
            Cry::Hoot => 0.7,
            Cry::Howl => 0.75,
            Cry::Yip => 0.75,
            Cry::Bugle => 0.56,
            Cry::Moo => 1.55,
            Cry::Neigh => 0.51,
            Cry::Trumpet => 0.6,
            Cry::Rumble => 0.9,
            Cry::Roar => 0.96,
            Cry::Chirp => 0.69,
            Cry::Snort => 1.02,
            Cry::Bark => 0.64,
            Cry::Bleat => 1.12,
            Cry::Grunt => 1.64,
            Cry::Honk => 1.04,
            Cry::Quack => 1.07,
            Cry::Screech => 0.60,
            Cry::Gull => 0.59,
            Cry::Pipe => 0.63,
            Cry::Chatter => 0.64,
            Cry::Drum => 1.80,
            Cry::Rattle => 0.36,
            Cry::Croak => 0.95,
            Cry::Trill => 0.70,
            Cry::Hiss => 0.43,
            Cry::Bellow => 1.19,
            Cry::Buzz => 0.60,
            Cry::Growl => 1.24,
            Cry::Scream => 0.57,
            Cry::Whoop => 0.71,
            Cry::Bray => 0.80,
            Cry::Blow => 0.86,
            Cry::Moan => 0.72,
            Cry::Groan => 1.23,
            Cry::Flap => 0.70,
            Cry::Splash => 0.68,
            Cry::Clash => 1.0,
            Cry::Thump => 0.64,
        }
    }

    /// The call for a caller of `size` (1 as the kind is made; 2 twice as
    /// big, lower and slower) and `seed`.
    pub fn pattern(self, size: f32, seed: u32) -> Pattern {
        let mut p = Pattern::default();
        let size = size.clamp(0.1, 10.0);
        let b = Build {
            k: size.powf(-0.6),
            tk: size.powf(0.2),
        };
        let mut r = Rng::new(seed ^ (self as u32).wrapping_mul(0x51ED_27A1));
        match self {
            Cry::Song => song(&mut p, &b, &mut r),
            Cry::Caw => caw(&mut p, &b, &mut r),
            Cry::Hoot => hoot(&mut p, &b, &mut r),
            Cry::Howl => howl(&mut p, &b, &mut r),
            Cry::Yip => yip(&mut p, &b, &mut r),
            Cry::Bugle => bugle(&mut p, &b, &mut r),
            Cry::Moo => moo(&mut p, &b, &mut r),
            Cry::Neigh => neigh(&mut p, &b, &mut r),
            Cry::Trumpet => trumpet(&mut p, &b, &mut r),
            Cry::Rumble => rumble(&mut p, &b, &mut r),
            Cry::Roar => roar(&mut p, &b, &mut r),
            Cry::Chirp => chirp(&mut p, &b, &mut r),
            Cry::Snort => snort(&mut p, &b, &mut r),
            Cry::Bark => bark(&mut p, &b, &mut r),
            Cry::Bleat => bleat(&mut p, &b, &mut r),
            Cry::Grunt => grunt(&mut p, &b, &mut r),
            Cry::Honk => honk(&mut p, &b, &mut r),
            Cry::Quack => quack(&mut p, &b, &mut r),
            Cry::Screech => screech(&mut p, &b, &mut r),
            Cry::Gull => gull(&mut p, &b, &mut r),
            Cry::Pipe => pipe(&mut p, &b, &mut r),
            Cry::Chatter => chatter(&mut p, &b, &mut r),
            Cry::Drum => drum(&mut p, &b, &mut r),
            Cry::Rattle => rattle(&mut p, &b, &mut r),
            Cry::Croak => croak(&mut p, &b, &mut r),
            Cry::Trill => trill(&mut p, &b, &mut r),
            Cry::Hiss => hiss(&mut p, &b, &mut r),
            Cry::Bellow => bellow(&mut p, &b, &mut r),
            Cry::Buzz => buzz(&mut p, &b, &mut r),
            Cry::Growl => growl(&mut p, &b, &mut r),
            Cry::Scream => scream(&mut p, &b, &mut r),
            Cry::Whoop => whoop(&mut p, &b, &mut r),
            Cry::Bray => bray(&mut p, &b, &mut r),
            Cry::Blow => blow(&mut p, &b, &mut r),
            Cry::Moan => moan(&mut p, &b, &mut r),
            Cry::Groan => groan(&mut p, &b, &mut r),
            Cry::Flap => flap(&mut p, &b, &mut r),
            Cry::Splash => splash(&mut p, &b, &mut r),
            Cry::Clash => clash(&mut p, &b, &mut r),
            Cry::Thump => thump(&mut p, &b, &mut r),
        }
        let t = self.trim();
        for s in &mut p.syl[..p.n] {
            s.amp *= t;
        }
        p.len = p.syl[..p.n].iter().map(|s| s.at + s.dur).fold(0.0, f32::max);
        p
    }
}

/// Reference level for squeezing real loudness into the game's: a call of
/// this many dB at 1 m plays at gain 1 two metres off.
const REF_DB: f32 = 95.0;
/// How much of the real spread between quiet and loud calls is kept. All
/// of it (1) and a lion drowns a songbird by 30 dB; this keeps about 20.
const SQUEEZE: f32 = 0.7;

/// The gain a call is posted at (`Noise::gain`) for a real level of `spl`
/// dB at 1 m, raised `boost` dB (a marquee call).
pub fn gain_of(spl: f32, boost: f32) -> f32 {
    let db = REF_DB + SQUEEZE * (spl - REF_DB) + boost;
    10f32.powf((db - REF_DB) / 20.0).min(4.0)
}

/// Excess loss over distance on top of spreading, dB per metre: ground,
/// grass and air together, about 5 dB a kilometre.
pub const EXCESS_DB_M: f32 = 0.005;

/// The lowpass the air puts on a call `d` metres off. High frequencies die
/// first: a songbird's top is gone at a hundred metres, a howl's body
/// carries a kilometre.
pub fn air_cut(d: f32) -> f32 {
    (24_000.0 / (1.0 + d / 30.0)).max(500.0)
}

/// The most syllables a call has.
pub const SYLS: usize = 24;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Src {
    #[default]
    Tone,
    Voiced,
    Rough,
    Hiss,
}

/// One syllable. Times in seconds from the start of the call, pitches and
/// formants in Hz.
#[derive(Clone, Copy, Debug)]
pub struct Syl {
    pub src: Src,
    pub at: f32,
    pub dur: f32,
    /// Pitch at the start, at `mid` (0 to 1 through it) and at the end,
    /// eased between on a log scale.
    pub hz: [f32; 3],
    pub mid: f32,
    pub attack: f32,
    pub release: f32,
    pub amp: f32,
    /// Vibrato: rate in Hz, depth as a fraction of the pitch.
    pub vib: [f32; 2],
    /// Pulsing: rate in Hz, depth 0 to 1, sharpness (1 a smooth wobble,
    /// higher shorter pulses with silence between: a trill, a cricket).
    pub trem: [f32; 3],
    /// How far each pulse's pitch falls across it, as a fraction: the
    /// downslur of each note in a trill.
    pub chirp: f32,
    /// Formant centres at the start and end, and their gains; all gains 0
    /// plays the source straight.
    pub form: [f32; 3],
    pub form_end: [f32; 3],
    pub fgain: [f32; 3],
    pub fq: f32,
    /// Tone: how much second and third harmonic. Voiced: how bright the
    /// pulses are before the formants.
    pub bright: f32,
    /// Noise in the voice, 0 to about 1.
    pub breath: f32,
    /// Period-to-period wobble in pitch, as a fraction.
    pub jitter: f32,
    /// Rough: how far out of step alternate periods are, 0 to 1.
    pub rough: f32,
    /// Carry on from the syllable before with its phase and filters, so the
    /// pitch runs on unbroken (set that one's release and this one's attack
    /// to 0).
    pub tie: bool,
}

impl Default for Syl {
    fn default() -> Self {
        Syl {
            src: Src::Tone,
            at: 0.0,
            dur: 0.2,
            hz: [440.0; 3],
            mid: 0.5,
            attack: 0.01,
            release: 0.03,
            amp: 1.0,
            vib: [0.0; 2],
            trem: [0.0, 0.0, 1.0],
            chirp: 0.0,
            form: [0.0; 3],
            form_end: [0.0; 3],
            fgain: [0.0; 3],
            fq: 5.0,
            bright: 0.2,
            breath: 0.0,
            jitter: 0.01,
            rough: 0.0,
            tie: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pattern {
    pub syl: [Syl; SYLS],
    pub n: usize,
    /// Seconds from the first syllable's start to the last one's end.
    pub len: f32,
}

impl Default for Pattern {
    fn default() -> Self {
        Pattern {
            syl: [Syl::default(); SYLS],
            n: 0,
            len: 0.0,
        }
    }
}

impl Pattern {
    fn push(&mut self, s: Syl) {
        if self.n < SYLS {
            self.syl[self.n] = s;
            self.n += 1;
        }
    }
}

/// A caller's scale: `k` on every frequency, `tk` on every time.
struct Build {
    k: f32,
    tk: f32,
}

impl Build {
    fn hz(&self, h: [f32; 3]) -> [f32; 3] {
        h.map(|x| x * self.k)
    }
    /// Formants scale less than pitch: the tract grows slower than the
    /// folds lengthen.
    fn form(&self, f: [f32; 3]) -> [f32; 3] {
        f.map(|x| x * self.k.powf(0.6))
    }
}

fn song(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // The individual's own pitch and pace.
    let base = r.range(2800.0, 4600.0);
    let pace = r.range(0.85, 1.15) * b.tk;
    let phrases = 2 + r.next() % 3;
    let mut t = 0.0;
    for i in 0..phrases {
        // A song drifts down a little toward its end.
        let f = base * r.range(0.75, 1.35) * (1.0 - 0.07 * i as f32);
        let tone = Syl {
            src: Src::Tone,
            bright: r.range(0.03, 0.15),
            breath: 0.02,
            jitter: 0.0,
            ..Syl::default()
        };
        match r.next() % 4 {
            // A run of slurred whistles, all the same way.
            0 => {
                let notes = 2 + r.next() % 4;
                let up = r.unit() < 0.35;
                let dur = r.range(0.06, 0.14) * pace;
                let gap = r.range(0.03, 0.07) * pace;
                let (a, z) = if up { (0.8, 1.2) } else { (1.25, 0.8) };
                for _ in 0..notes {
                    p.push(Syl {
                        at: t,
                        dur,
                        hz: b.hz([f * a, f, f * z]),
                        attack: 0.008,
                        release: 0.02,
                        ..tone
                    });
                    t += dur + gap;
                }
            }
            // A trill: one long syllable pulsed into notes.
            1 => {
                let dur = r.range(0.4, 0.9) * pace;
                p.push(Syl {
                    at: t,
                    dur,
                    hz: b.hz([f, f * 0.95, f * 0.85]),
                    trem: [r.range(14.0, 28.0) / pace, 1.0, 6.0],
                    chirp: r.range(0.1, 0.3),
                    attack: 0.02,
                    release: 0.05,
                    ..tone
                });
                t += dur;
            }
            // A buzz: a fast flutter with air in it.
            2 => {
                let dur = r.range(0.25, 0.5) * pace;
                p.push(Syl {
                    at: t,
                    dur,
                    hz: b.hz([f * 0.85, f * 0.9, f * 0.88]),
                    trem: [r.range(90.0, 160.0), 0.8, 1.5],
                    bright: 0.5,
                    breath: 0.12,
                    attack: 0.02,
                    release: 0.04,
                    amp: 0.7,
                    ..tone
                });
                t += dur;
            }
            // Tee-oo: a high note and a low one.
            _ => {
                let d1 = r.range(0.12, 0.2) * pace;
                let d2 = r.range(0.14, 0.24) * pace;
                p.push(Syl {
                    at: t,
                    dur: d1,
                    hz: b.hz([f * 1.15, f * 1.2, f * 1.1]),
                    attack: 0.01,
                    release: 0.02,
                    ..tone
                });
                t += d1 + 0.02 * pace;
                p.push(Syl {
                    at: t,
                    dur: d2,
                    hz: b.hz([f * 0.8, f * 0.78, f * 0.7]),
                    attack: 0.01,
                    release: 0.04,
                    vib: [r.range(20.0, 40.0), 0.02],
                    ..tone
                });
                t += d2;
            }
        }
        t += r.range(0.05, 0.12) * pace;
    }
}

fn caw(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 2 + r.next() % 3;
    let f = r.range(440.0, 560.0);
    let mut t = 0.0;
    for i in 0..n {
        let dur = r.range(0.25, 0.4) * b.tk;
        // The caws of a run fall a little and weaken.
        let fall = 1.0 - 0.04 * i as f32;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f * 0.85 * fall, f * 1.05 * fall, f * 0.8 * fall]),
            mid: 0.3,
            attack: 0.02,
            release: 0.07,
            amp: 1.0 - 0.1 * i as f32,
            form: b.form([1100.0, 1700.0, 3000.0]),
            form_end: b.form([950.0, 1500.0, 2800.0]),
            fgain: [1.0, 0.8, 0.4],
            fq: 4.0,
            bright: 0.7,
            breath: 0.35,
            jitter: 0.04,
            rough: 0.35,
            ..Syl::default()
        });
        t += dur + r.range(0.22, 0.34) * b.tk;
    }
}

fn hoot(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // hoo, hoo-hoo, hoooo, hoo, hoo: the great horned owl's rhythm.
    let f = r.range(280.0, 340.0);
    let notes: [(f32, f32, f32); 6] = [(0.3, 0.25, 0.8), (0.12, 0.06, 0.7), (0.12, 0.12, 0.7), (0.6, 0.25, 1.0), (0.35, 0.2, 0.85), (0.35, 0.0, 0.8)];
    let mut t = 0.0;
    for (dur, gap, amp) in notes {
        let dur = dur * b.tk;
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur,
            hz: b.hz([f * 0.95, f * 1.04, f * 0.92]),
            attack: 0.05,
            release: (dur * 0.4).min(0.12),
            amp,
            bright: 0.15,
            breath: 0.08,
            jitter: 0.0,
            ..Syl::default()
        });
        t += dur + gap * b.tk;
    }
}

fn howl(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let peak = r.range(700.0, 1000.0);
    let rise = r.range(0.9, 1.4) * b.tk;
    let fall = r.range(1.0, 1.8) * b.tk;
    let voice = Syl {
        src: Src::Tone,
        bright: 0.35,
        breath: 0.05,
        jitter: 0.0,
        ..Syl::default()
    };
    // Up from low into a long held note...
    p.push(Syl {
        at: 0.0,
        dur: rise,
        hz: b.hz([peak * 0.42, peak, peak * 0.97]),
        mid: 0.25,
        attack: 0.25,
        release: 0.0,
        vib: [r.range(4.0, 6.0), 0.012],
        ..voice
    });
    // ... and down, wavering, to where it breaks off.
    p.push(Syl {
        at: rise,
        dur: fall,
        hz: b.hz([peak * 0.97, peak * 0.9, peak * r.range(0.55, 0.7)]),
        mid: 0.5,
        attack: 0.0,
        release: 0.45,
        vib: [r.range(4.0, 6.0), 0.018],
        tie: true,
        ..voice
    });
}

fn yip(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 6 + r.next() % 9;
    let mut t = 0.0;
    for _ in 0..n {
        let f = r.range(850.0, 1300.0);
        let dur = r.range(0.06, 0.12) * b.tk;
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur,
            hz: b.hz([f * 0.8, f * 1.15, f * 0.75]),
            mid: 0.4,
            attack: 0.008,
            release: 0.03,
            amp: r.range(0.6, 1.0),
            bright: 0.5,
            breath: 0.08,
            jitter: 0.0,
            ..Syl::default()
        });
        t += dur + r.range(0.04, 0.15) * b.tk;
    }
    // Often a short wavering howl to finish.
    if r.unit() < 0.6 {
        let f = r.range(850.0, 1100.0);
        p.push(Syl {
            src: Src::Tone,
            at: t + 0.05,
            dur: r.range(0.5, 0.8) * b.tk,
            hz: b.hz([f * 0.9, f * 1.05, f * 0.65]),
            attack: 0.05,
            release: 0.2,
            vib: [r.range(8.0, 11.0), 0.06],
            bright: 0.5,
            breath: 0.08,
            jitter: 0.0,
            ..Syl::default()
        });
    }
}

fn bugle(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let top = r.range(1400.0, 2000.0);
    let growl_form = b.form([400.0, 900.0, 1800.0]);
    // The growl it starts on.
    p.push(Syl {
        src: Src::Rough,
        at: 0.0,
        dur: 0.5 * b.tk,
        hz: b.hz([140.0, 180.0, 220.0]),
        attack: 0.08,
        release: 0.1,
        amp: 0.5,
        form: growl_form,
        form_end: growl_form,
        fgain: [1.0, 0.7, 0.4],
        fq: 3.0,
        bright: 0.6,
        breath: 0.25,
        jitter: 0.04,
        rough: 0.5,
        ..Syl::default()
    });
    let whistle = Syl {
        src: Src::Tone,
        bright: 0.25,
        breath: 0.18,
        jitter: 0.0,
        ..Syl::default()
    };
    let up = 0.5 * b.tk;
    let hold = r.range(1.0, 1.8) * b.tk;
    let t0 = 0.35 * b.tk;
    // Up into the scream, held, then the break downward.
    p.push(Syl {
        at: t0,
        dur: up,
        hz: b.hz([top * 0.35, top * 0.8, top]),
        mid: 0.6,
        attack: 0.08,
        release: 0.0,
        ..whistle
    });
    p.push(Syl {
        at: t0 + up,
        dur: hold,
        hz: b.hz([top, top * 1.03, top * 0.92]),
        attack: 0.0,
        release: 0.0,
        vib: [r.range(5.0, 7.0), 0.01],
        tie: true,
        ..whistle
    });
    p.push(Syl {
        at: t0 + up + hold,
        dur: 0.25 * b.tk,
        hz: b.hz([top * 0.92, top * 0.6, top * 0.3]),
        attack: 0.0,
        release: 0.08,
        tie: true,
        ..whistle
    });
    // The low voice under the whistle the whole way.
    p.push(Syl {
        src: Src::Voiced,
        at: t0,
        dur: up + hold + 0.2 * b.tk,
        hz: b.hz([150.0, 165.0, 130.0]),
        attack: 0.15,
        release: 0.15,
        amp: 0.3,
        form: b.form([300.0, 700.0, 1500.0]),
        form_end: b.form([320.0, 750.0, 1500.0]),
        fgain: [1.0, 0.6, 0.3],
        fq: 3.0,
        bright: 0.5,
        breath: 0.1,
        jitter: 0.02,
        ..Syl::default()
    });
    // Then the chuckles: a few low grunts.
    let mut t = t0 + up + hold + 0.45 * b.tk;
    for i in 0..(2 + r.next() % 4) {
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur: 0.12 * b.tk,
            hz: b.hz([110.0, 125.0, 100.0]),
            attack: 0.01,
            release: 0.05,
            amp: 0.7 - 0.08 * i as f32,
            form: b.form([350.0, 800.0, 1600.0]),
            form_end: b.form([350.0, 800.0, 1600.0]),
            fgain: [1.0, 0.7, 0.3],
            fq: 3.0,
            bright: 0.6,
            breath: 0.3,
            jitter: 0.05,
            rough: 0.6,
            ..Syl::default()
        });
        t += r.range(0.22, 0.32) * b.tk;
    }
}

fn moo(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(95.0, 130.0);
    let open = r.range(0.5, 0.8) * b.tk;
    let close = r.range(0.7, 1.2) * b.tk;
    let voice = Syl {
        src: Src::Voiced,
        fq: 5.0,
        fgain: [1.0, 0.7, 0.25],
        bright: 0.4,
        breath: 0.04,
        jitter: 0.015,
        vib: [5.0, 0.01],
        ..Syl::default()
    };
    // Mm, opening to aa...
    p.push(Syl {
        at: 0.0,
        dur: open,
        hz: b.hz([f, f * 1.2, f * 1.3]),
        attack: 0.12,
        release: 0.0,
        form: b.form([250.0, 650.0, 2200.0]),
        form_end: b.form([700.0, 1150.0, 2500.0]),
        ..voice
    });
    // ... and closing to oo as it falls.
    p.push(Syl {
        at: open,
        dur: close,
        hz: b.hz([f * 1.3, f * 1.25, f * 0.9]),
        attack: 0.0,
        release: 0.3,
        form: b.form([700.0, 1150.0, 2500.0]),
        form_end: b.form([350.0, 800.0, 2300.0]),
        tie: true,
        ..voice
    });
}

fn neigh(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(900.0, 1100.0);
    let first = r.range(0.8, 1.1) * b.tk;
    let second = r.range(0.4, 0.6) * b.tk;
    let voice = Syl {
        src: Src::Voiced,
        form: b.form([1100.0, 2200.0, 3500.0]),
        form_end: b.form([1000.0, 2000.0, 3300.0]),
        fgain: [1.0, 0.7, 0.4],
        fq: 3.0,
        bright: 0.6,
        breath: 0.15,
        jitter: 0.02,
        ..Syl::default()
    };
    // The high whinny, shaking as it comes down.
    p.push(Syl {
        at: 0.0,
        dur: first,
        hz: b.hz([f, f * 0.97, f * 0.62]),
        mid: 0.3,
        attack: 0.04,
        release: 0.0,
        vib: [r.range(11.0, 14.0), 0.07],
        ..voice
    });
    p.push(Syl {
        at: first,
        dur: second,
        hz: b.hz([f * 0.62, f * 0.48, f * 0.38]),
        attack: 0.0,
        release: 0.2,
        vib: [r.range(8.0, 10.0), 0.09],
        tie: true,
        ..voice
    });
    // The lower voice a horse has at the same time.
    p.push(Syl {
        at: 0.05,
        dur: first + second - 0.1,
        hz: b.hz([420.0, 400.0, 300.0]),
        attack: 0.05,
        release: 0.2,
        amp: 0.3,
        form: b.form([500.0, 1200.0, 2500.0]),
        form_end: b.form([450.0, 1100.0, 2400.0]),
        vib: [9.0, 0.04],
        ..voice
    });
    // A snort or two after.
    let mut t = first + second + 0.15;
    for _ in 0..(1 + r.next() % 2) {
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur: 0.15 * b.tk,
            attack: 0.005,
            release: 0.1,
            amp: 0.45,
            form: b.form([700.0, 1500.0, 3000.0]),
            form_end: b.form([600.0, 1300.0, 2800.0]),
            fgain: [0.7, 1.0, 0.5],
            fq: 2.0,
            ..Syl::default()
        });
        t += 0.22 * b.tk;
    }
}

fn trumpet(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let blasts = 1 + (r.unit() < 0.4) as u32;
    let mut t = 0.0;
    for i in 0..blasts {
        let f = r.range(380.0, 480.0);
        let dur = r.range(0.9, 1.5) * b.tk * if i > 0 { 0.6 } else { 1.0 };
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f * 0.85, f * 1.1, f * 0.95]),
            mid: 0.25,
            attack: 0.03,
            release: 0.25,
            amp: 1.0 - 0.25 * i as f32,
            vib: [r.range(6.0, 8.0), 0.03],
            form: b.form([900.0, 1700.0, 2800.0]),
            form_end: b.form([850.0, 1600.0, 2700.0]),
            fgain: [1.0, 0.9, 0.6],
            fq: 3.0,
            bright: 1.0,
            breath: 0.4,
            jitter: 0.05,
            rough: 0.4,
            ..Syl::default()
        });
        t += dur + r.range(0.25, 0.45);
    }
}

fn rumble(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(15.0, 22.0);
    p.push(Syl {
        src: Src::Voiced,
        at: 0.0,
        dur: r.range(2.5, 4.0) * b.tk,
        hz: b.hz([f, f * 1.25, f * 0.9]),
        attack: 0.4,
        release: 0.8,
        form: b.form([70.0, 140.0, 260.0]),
        form_end: b.form([80.0, 150.0, 260.0]),
        fgain: [1.0, 0.6, 0.3],
        fq: 2.0,
        bright: 0.3,
        breath: 0.05,
        jitter: 0.03,
        ..Syl::default()
    });
}

fn roar(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let form = b.form([350.0, 900.0, 1700.0]);
    let mut t = 0.0;
    // Two moans to begin.
    for _ in 0..2 {
        let dur = r.range(0.8, 1.0) * b.tk;
        p.push(Syl {
            src: Src::Voiced,
            at: t,
            dur,
            hz: b.hz([140.0, 180.0, 130.0]),
            attack: 0.15,
            release: 0.3,
            amp: 0.4,
            form: b.form([300.0, 700.0, 1300.0]),
            form_end: b.form([300.0, 700.0, 1300.0]),
            fgain: [1.0, 0.6, 0.3],
            fq: 4.0,
            bright: 0.5,
            breath: 0.2,
            jitter: 0.03,
            ..Syl::default()
        });
        t += dur + r.range(0.6, 0.9) * b.tk;
    }
    // The full roars, louder then easing.
    let full = 3 + r.next() % 3;
    for i in 0..full {
        let dur = r.range(1.2, 1.7) * b.tk;
        let swell = 1.0 - (i as f32 - 1.0).abs() * 0.12;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([180.0, 220.0, 120.0]),
            mid: 0.3,
            attack: 0.1,
            release: 0.35,
            amp: swell,
            form,
            form_end: b.form([320.0, 800.0, 1600.0]),
            fgain: [1.0, 0.7, 0.4],
            fq: 3.0,
            bright: 0.8,
            breath: 0.35,
            jitter: 0.05,
            rough: 0.6,
            ..Syl::default()
        });
        t += dur + r.range(0.5, 0.8) * b.tk;
    }
    // Then the grunts, coming quicker and softer.
    let grunts = (6 + r.next() % 7) as usize;
    let mut gap = 0.9 * b.tk;
    for i in 0..grunts {
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur: 0.25 * b.tk,
            hz: b.hz([130.0, 140.0, 110.0]),
            attack: 0.02,
            release: 0.1,
            amp: 0.6 * (1.0 - 0.05 * i as f32),
            form,
            form_end: form,
            fgain: [1.0, 0.7, 0.3],
            fq: 3.0,
            bright: 0.7,
            breath: 0.35,
            jitter: 0.05,
            rough: 0.5,
            ..Syl::default()
        });
        t += 0.25 * b.tk + gap;
        gap = (gap * 0.9).max(0.45 * b.tk);
    }
}

fn chirp(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(4300.0, 4900.0);
    let n = 10 + r.next() % 7;
    let every = r.range(0.33, 0.45) * b.tk;
    let pulses = 3.0 + (r.next() % 2) as f32;
    let rate = r.range(28.0, 36.0);
    for i in 0..n {
        p.push(Syl {
            src: Src::Tone,
            at: i as f32 * every,
            dur: pulses / rate,
            hz: b.hz([f, f, f * 0.98]),
            attack: 0.002,
            release: 0.004,
            trem: [rate, 1.0, 2.0],
            chirp: 0.02,
            bright: 0.1,
            jitter: 0.0,
            ..Syl::default()
        });
    }
}

fn snort(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + (r.unit() < 0.5) as u32;
    let mut t = 0.0;
    for i in 0..n {
        let dur = r.range(0.25, 0.4) * b.tk;
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur,
            attack: 0.005,
            release: dur * 0.7,
            amp: 1.0 - 0.3 * i as f32,
            form: b.form([600.0, 1400.0, 2800.0]),
            form_end: b.form([500.0, 1200.0, 2600.0]),
            fgain: [0.6, 1.0, 0.5],
            fq: 2.0,
            ..Syl::default()
        });
        t += dur + r.range(0.4, 0.8);
    }
}

fn bark(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + r.next() % 5;
    let f = r.range(600.0, 800.0);
    let mut t = 0.0;
    for i in 0..n {
        let dur = r.range(0.12, 0.2) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f * 1.05, f * 1.2, f * 0.7]),
            mid: 0.3,
            attack: 0.006,
            release: 0.06,
            amp: 1.0 - 0.08 * i as f32,
            form: b.form([900.0, 1800.0, 3000.0]),
            form_end: b.form([800.0, 1600.0, 2800.0]),
            fgain: [1.0, 0.8, 0.4],
            fq: 3.0,
            bright: 0.8,
            breath: 0.3,
            jitter: 0.04,
            rough: 0.5,
            ..Syl::default()
        });
        t += dur + r.range(0.25, 0.5) * b.tk;
    }
}

fn bleat(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + (r.unit() < 0.4) as u32;
    let mut t = 0.0;
    for i in 0..n {
        let f = r.range(180.0, 260.0);
        let dur = r.range(0.6, 1.0) * b.tk * if i > 0 { 0.7 } else { 1.0 };
        let q = r.range(6.5, 9.0) / b.tk;
        p.push(Syl {
            src: Src::Voiced,
            at: t,
            dur,
            hz: b.hz([f, f * 1.08, f * 0.9]),
            mid: 0.3,
            attack: 0.05,
            release: 0.2,
            amp: 1.0 - 0.2 * i as f32,
            // The quaver: pitch and loudness shaking together.
            vib: [q, 0.05],
            trem: [q, 0.45, 1.0],
            form: b.form([500.0, 1500.0, 2600.0]),
            form_end: b.form([750.0, 1700.0, 2600.0]),
            fgain: [1.0, 0.7, 0.35],
            fq: 4.0,
            bright: 0.6,
            breath: 0.1,
            jitter: 0.03,
            ..Syl::default()
        });
        t += dur + r.range(0.4, 0.9) * b.tk;
    }
}

fn grunt(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 2 + r.next() % 5;
    let f = r.range(90.0, 130.0);
    let mut t = 0.0;
    for _ in 0..n {
        let dur = r.range(0.1, 0.25) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f, f * 1.1, f * 0.85]),
            attack: 0.015,
            release: 0.06,
            amp: r.range(0.7, 1.0),
            form: b.form([400.0, 1000.0, 2200.0]),
            form_end: b.form([380.0, 950.0, 2100.0]),
            fgain: [1.0, 0.6, 0.25],
            fq: 4.0,
            bright: 0.6,
            breath: 0.4,
            jitter: 0.05,
            rough: 0.5,
            ..Syl::default()
        });
        t += dur + r.range(0.15, 0.6) * b.tk;
    }
}

fn honk(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + r.next() % 4;
    let mut t = 0.0;
    for i in 0..n {
        let f = r.range(400.0, 550.0);
        let dur = r.range(0.18, 0.3) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            // The break up into it: "h-onk".
            hz: b.hz([f * 0.75, f * 1.1, f * 1.0]),
            mid: 0.25,
            attack: 0.01,
            release: 0.06,
            amp: 1.0 - 0.07 * i as f32,
            form: b.form([800.0, 1500.0, 2600.0]),
            form_end: b.form([750.0, 1400.0, 2500.0]),
            fgain: [1.0, 0.9, 0.4],
            fq: 6.0,
            bright: 0.8,
            breath: 0.1,
            jitter: 0.02,
            rough: 0.15,
            ..Syl::default()
        });
        t += dur + r.range(0.25, 0.5) * b.tk;
    }
}

fn quack(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // QUACK quack quack quack: louder first, falling and quickening.
    let n = 3 + r.next() % 5;
    let f = r.range(250.0, 320.0);
    let mut t = 0.0;
    let mut gap = 0.18 * b.tk;
    for i in 0..n {
        let dur = r.range(0.12, 0.18) * b.tk * if i == 0 { 1.4 } else { 1.0 };
        let fall = 1.0 - 0.05 * i as f32;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f * fall, f * 1.1 * fall, f * 0.85 * fall]),
            mid: 0.3,
            attack: 0.008,
            release: 0.05,
            amp: (1.0 - 0.12 * i as f32).max(0.3),
            form: b.form([1100.0, 1800.0, 2800.0]),
            form_end: b.form([950.0, 1600.0, 2600.0]),
            fgain: [1.0, 0.9, 0.4],
            fq: 5.0,
            bright: 0.8,
            breath: 0.3,
            jitter: 0.04,
            rough: 0.4,
            ..Syl::default()
        });
        t += dur + gap;
        gap = (gap * 0.85).max(0.08 * b.tk);
    }
}

fn screech(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(2200.0, 3000.0);
    if r.unit() < 0.65 {
        // Kee-eeeer: one long hoarse scream falling away.
        let dur = r.range(1.0, 1.6) * b.tk;
        p.push(Syl {
            src: Src::Tone,
            at: 0.0,
            dur,
            hz: b.hz([f * 0.85, f * 1.05, f * 0.65]),
            mid: 0.2,
            attack: 0.05,
            release: 0.35,
            vib: [r.range(25.0, 35.0), 0.015],
            bright: 0.6,
            breath: 0.5,
            jitter: 0.0,
            ..Syl::default()
        });
        // The rasp through it.
        p.push(Syl {
            src: Src::Hiss,
            at: 0.03,
            dur: dur * 0.9,
            attack: 0.05,
            release: 0.35,
            amp: 0.45,
            form: b.hz([f, f * 1.5, f * 2.1]),
            form_end: b.hz([f * 0.7, f * 1.2, f * 1.8]),
            fgain: [1.0, 0.4, 0.2],
            fq: 5.0,
            ..Syl::default()
        });
    } else {
        // Kek-kek-kek: a falcon's cackle.
        let n = 4 + r.next() % 6;
        let mut t = 0.0;
        for _ in 0..n {
            let dur = r.range(0.1, 0.14) * b.tk;
            p.push(Syl {
                src: Src::Rough,
                at: t,
                dur,
                hz: b.hz([f * 0.5, f * 0.55, f * 0.45]),
                attack: 0.005,
                release: 0.04,
                form: b.form([1800.0, 3000.0, 4500.0]),
                form_end: b.form([1700.0, 2900.0, 4300.0]),
                fgain: [1.0, 0.7, 0.3],
                fq: 4.0,
                bright: 0.8,
                breath: 0.3,
                jitter: 0.03,
                rough: 0.4,
                ..Syl::default()
            });
            t += dur + r.range(0.06, 0.1) * b.tk;
        }
    }
}

fn gull(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // The long call: a couple of drawn-out cries, then a run of yelps.
    let f = r.range(1300.0, 1700.0);
    let voice = Syl {
        src: Src::Tone,
        mid: 0.35,
        attack: 0.02,
        release: 0.06,
        vib: [r.range(22.0, 30.0), 0.02],
        bright: 0.6,
        breath: 0.25,
        jitter: 0.0,
        ..Syl::default()
    };
    let mut t = 0.0;
    for _ in 0..(1 + r.next() % 2) {
        let dur = r.range(0.35, 0.5) * b.tk;
        p.push(Syl { at: t, dur, hz: b.hz([f * 0.65, f * 1.1, f * 0.8]), ..voice });
        t += dur + 0.15 * b.tk;
    }
    let yelps = 4 + r.next() % 6;
    for i in 0..yelps {
        let dur = r.range(0.15, 0.2) * b.tk;
        let g = f * (1.0 - 0.03 * i as f32);
        p.push(Syl {
            at: t,
            dur,
            hz: b.hz([g * 0.7, g * 1.15, g * 0.85]),
            amp: 1.0 - 0.05 * i as f32,
            ..voice
        });
        t += dur + r.range(0.08, 0.13) * b.tk;
    }
}

fn pipe(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let f = r.range(2500.0, 3800.0);
    let tone = Syl {
        src: Src::Tone,
        attack: 0.008,
        release: 0.03,
        bright: 0.1,
        breath: 0.03,
        jitter: 0.0,
        ..Syl::default()
    };
    let mut t = 0.0;
    match r.next() % 4 {
        // Peep, peep: one upslurred note, a few times.
        0 => {
            for _ in 0..(1 + r.next() % 4) {
                let dur = r.range(0.08, 0.14) * b.tk;
                p.push(Syl { at: t, dur, hz: b.hz([f * 0.8, f * 1.05, f * 1.1]), mid: 0.6, ..tone });
                t += dur + r.range(0.25, 0.6) * b.tk;
            }
        }
        // Kill-deer: a high note and a falling one, over again.
        1 => {
            for _ in 0..(2 + r.next() % 3) {
                let d1 = 0.1 * b.tk;
                let d2 = 0.2 * b.tk;
                p.push(Syl { at: t, dur: d1, hz: b.hz([f * 1.1, f * 1.15, f * 1.1]), ..tone });
                p.push(Syl { at: t + d1 + 0.04 * b.tk, dur: d2, hz: b.hz([f * 1.05, f, f * 0.75]), mid: 0.3, ..tone });
                t += d1 + d2 + r.range(0.35, 0.55) * b.tk;
            }
        }
        // A piping run that quickens and runs down.
        2 => {
            let n = 8 + r.next() % 10;
            let mut gap = 0.16 * b.tk;
            for i in 0..n {
                let dur = 0.07 * b.tk;
                let g = f * (1.0 - 0.012 * i as f32);
                p.push(Syl { at: t, dur, hz: b.hz([g * 0.95, g * 1.02, g * 0.9]), ..tone });
                t += dur + gap;
                gap = (gap * 0.9).max(0.04 * b.tk);
            }
        }
        // Chi-CA-go: three notes, the middle one high.
        _ => {
            for (k, d) in [(0.8, 0.1), (1.15, 0.16), (0.85, 0.12)] {
                let dur = d * b.tk;
                p.push(Syl { at: t, dur, hz: b.hz([f * k * 0.9, f * k, f * k * 0.95]), ..tone });
                t += dur + 0.05 * b.tk;
            }
        }
    }
}

fn chatter(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // A fast run of harsh notes, in one or two bursts.
    let n = 1 + (r.unit() < 0.5) as u32;
    let mut t = 0.0;
    for _ in 0..n {
        let f = r.range(900.0, 1300.0);
        let dur = r.range(0.8, 2.0) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f, f * 1.05, f * 0.9]),
            attack: 0.02,
            release: 0.1,
            trem: [r.range(12.0, 16.0) / b.tk, 1.0, 3.0],
            chirp: 0.15,
            form: b.form([2000.0, 3500.0, 5000.0]),
            form_end: b.form([1900.0, 3300.0, 4800.0]),
            fgain: [1.0, 0.7, 0.4],
            fq: 3.0,
            bright: 0.9,
            breath: 0.5,
            jitter: 0.05,
            rough: 0.7,
            ..Syl::default()
        });
        t += dur + r.range(0.4, 1.0) * b.tk;
    }
}

fn drum(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Bill on wood: bursts of knocks too quick to count, through the
    // trunk's ring.
    let n = 1 + r.next() % 3;
    let ring = r.range(0.85, 1.2);
    let mut t = 0.0;
    for _ in 0..n {
        let dur = r.range(0.5, 1.2);
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur,
            attack: 0.0,
            release: 0.15,
            trem: [r.range(16.0, 24.0), 1.0, 40.0],
            form: b.form([1000.0 * ring, 2200.0 * ring, 4000.0 * ring]),
            form_end: b.form([1000.0 * ring, 2200.0 * ring, 4000.0 * ring]),
            fgain: [1.0, 0.6, 0.3],
            fq: 6.0,
            ..Syl::default()
        });
        t += dur + r.range(1.0, 2.5);
    }
}

fn rattle(p: &mut Pattern, _b: &Build, r: &mut Rng) {
    let dur = r.range(1.5, 3.5);
    p.push(Syl {
        src: Src::Hiss,
        at: 0.0,
        dur,
        attack: 0.2,
        release: 0.3,
        trem: [r.range(45.0, 60.0), 0.9, 4.0],
        form: [4000.0, 7000.0, 10_000.0],
        form_end: [4000.0, 7000.0, 10_000.0],
        fgain: [0.8, 1.0, 0.7],
        fq: 1.5,
        ..Syl::default()
    });
}

fn croak(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Rib-bit: two pulsed notes, the second longer; a few of them.
    let n = 1 + r.next() % 3;
    let f = r.range(400.0, 600.0);
    let rate = r.range(60.0, 90.0) / b.tk;
    let mut t = 0.0;
    for _ in 0..n {
        for (k, d) in [(1.0, 0.08), (0.95, 0.13)] {
            let dur = d * b.tk;
            p.push(Syl {
                src: Src::Voiced,
                at: t,
                dur,
                hz: b.hz([f * k, f * k * 1.05, f * k * 0.95]),
                attack: 0.005,
                release: 0.02,
                trem: [rate, 1.0, 2.0],
                form: b.form([1200.0, 2000.0, 3000.0]),
                form_end: b.form([1100.0, 1900.0, 2900.0]),
                fgain: [1.0, 0.7, 0.3],
                fq: 4.0,
                bright: 0.7,
                breath: 0.1,
                jitter: 0.03,
                ..Syl::default()
            });
            t += dur + 0.04 * b.tk;
        }
        t += r.range(0.4, 0.8) * b.tk;
    }
}

fn trill(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // One long even trill: a toad, or a nightjar churring.
    let f = r.range(1700.0, 2100.0);
    let dur = r.range(2.5, 6.0) * b.tk;
    p.push(Syl {
        src: Src::Tone,
        at: 0.0,
        dur,
        hz: b.hz([f * 0.98, f, f * 0.97]),
        attack: 0.3,
        release: 0.4,
        trem: [r.range(35.0, 45.0) / b.tk, 1.0, 3.0],
        bright: 0.3,
        breath: 0.04,
        jitter: 0.0,
        ..Syl::default()
    });
}

fn hiss(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + (r.unit() < 0.4) as u32;
    let mut t = 0.0;
    for i in 0..n {
        let dur = r.range(0.6, 1.5) * b.tk;
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur,
            attack: 0.05,
            release: dur * 0.5,
            amp: 1.0 - 0.3 * i as f32,
            form: b.form([2500.0, 5000.0, 8000.0]),
            form_end: b.form([2300.0, 4800.0, 7800.0]),
            fgain: [0.6, 1.0, 0.6],
            fq: 1.5,
            ..Syl::default()
        });
        t += dur + r.range(0.3, 0.7);
    }
}

fn bellow(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // A reptile's (or a great beast's) bellow: long, low, ragged.
    let n = 1 + r.next() % 3;
    let mut t = 0.0;
    for i in 0..n {
        let f = r.range(30.0, 45.0);
        let dur = r.range(1.5, 2.5) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f, f * 1.15, f * 0.9]),
            mid: 0.3,
            attack: 0.2,
            release: 0.5,
            amp: 1.0 - 0.1 * i as f32,
            vib: [r.range(5.0, 8.0), 0.04],
            form: b.form([250.0, 600.0, 1200.0]),
            form_end: b.form([230.0, 550.0, 1100.0]),
            fgain: [1.0, 0.7, 0.35],
            fq: 3.0,
            bright: 0.8,
            breath: 0.3,
            jitter: 0.06,
            rough: 0.6,
            ..Syl::default()
        });
        t += dur + r.range(1.0, 1.6) * b.tk;
    }
}

fn buzz(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Wings: a hum that wavers as the insect turns.
    let f = r.range(190.0, 240.0);
    p.push(Syl {
        src: Src::Voiced,
        at: 0.0,
        dur: r.range(1.0, 2.5),
        hz: b.hz([f, f * 1.04, f * 0.97]),
        attack: 0.3,
        release: 0.4,
        vib: [r.range(3.0, 6.0), 0.04],
        trem: [r.range(2.0, 4.0), 0.4, 1.0],
        bright: 0.9,
        breath: 0.05,
        jitter: 0.01,
        ..Syl::default()
    });
}

fn growl(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let form = b.form([300.0, 800.0, 1600.0]);
    let base = Syl {
        src: Src::Rough,
        fgain: [1.0, 0.6, 0.3],
        fq: 3.0,
        bright: 0.7,
        breath: 0.4,
        jitter: 0.06,
        rough: 0.7,
        form,
        form_end: form,
        ..Syl::default()
    };
    let f = r.range(70.0, 100.0);
    if r.unit() < 0.7 {
        // One or two long growls, trembling.
        let mut t = 0.0;
        for _ in 0..(1 + (r.unit() < 0.4) as u32) {
            let dur = r.range(1.0, 2.0) * b.tk;
            p.push(Syl {
                at: t,
                dur,
                hz: b.hz([f, f * 1.1, f * 0.9]),
                attack: 0.15,
                release: 0.3,
                trem: [r.range(6.0, 10.0), 0.3, 1.0],
                ..base
            });
            t += dur + r.range(0.4, 0.8) * b.tk;
        }
    } else {
        // Sawing: grunts in and out, a big cat's call through the trees.
        let n = 5 + r.next() % 5;
        let mut t = 0.0;
        for i in 0..n {
            let dur = 0.2 * b.tk;
            let out = i % 2 == 0;
            p.push(Syl {
                at: t,
                dur,
                hz: b.hz(if out { [f * 1.4, f * 1.5, f * 1.2] } else { [f * 1.1, f * 1.15, f] }),
                attack: 0.02,
                release: 0.08,
                amp: if out { 1.0 } else { 0.6 },
                ..base
            });
            t += dur + r.range(0.2, 0.3) * b.tk;
        }
    }
}

fn scream(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + r.next() % 3;
    let mut t = 0.0;
    for i in 0..n {
        let f = r.range(500.0, 800.0);
        let dur = r.range(0.8, 1.6) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f * 0.8, f * 1.2, f * 0.6]),
            mid: 0.3,
            attack: 0.06,
            release: 0.35,
            amp: 1.0 - 0.15 * i as f32,
            vib: [r.range(5.0, 7.0), 0.03],
            form: b.form([1100.0, 2000.0, 3200.0]),
            form_end: b.form([900.0, 1800.0, 3000.0]),
            fgain: [1.0, 0.8, 0.4],
            fq: 3.0,
            bright: 0.8,
            breath: 0.4,
            jitter: 0.04,
            rough: 0.5,
            ..Syl::default()
        });
        t += dur + r.range(1.0, 2.0) * b.tk;
    }
}

fn whoop(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Ooo-WHUP, rising, over and over with long gaps.
    let n = 3 + r.next() % 4;
    let mut t = 0.0;
    for i in 0..n {
        let f = r.range(300.0, 400.0);
        let dur = r.range(1.0, 1.5) * b.tk;
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur,
            hz: b.hz([f * 0.6, f * 0.9, f * 1.6]),
            mid: 0.7,
            attack: 0.15,
            release: 0.15,
            amp: 1.0 - 0.06 * i as f32,
            bright: 0.4,
            breath: 0.1,
            jitter: 0.0,
            ..Syl::default()
        });
        t += dur + r.range(1.2, 2.5) * b.tk;
    }
}

fn bray(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Hee on the breath in, haw on the breath out, swelling and fading.
    let pairs = 3 + r.next() % 4;
    let hee = r.range(600.0, 800.0);
    let haw = r.range(160.0, 200.0);
    let mut t = 0.0;
    for i in 0..pairs {
        let u = i as f32 / (pairs - 1).max(1) as f32;
        let amp = 0.6 + 0.4 * (PI * u).sin();
        let d1 = 0.35 * b.tk;
        p.push(Syl {
            src: Src::Voiced,
            at: t,
            dur: d1,
            hz: b.hz([hee * 0.8, hee, hee * 1.1]),
            attack: 0.04,
            release: 0.05,
            amp: amp * 0.7,
            form: b.form([600.0, 2200.0, 3000.0]),
            form_end: b.form([600.0, 2200.0, 3000.0]),
            fgain: [0.8, 1.0, 0.5],
            fq: 4.0,
            bright: 0.6,
            breath: 0.3,
            jitter: 0.03,
            ..Syl::default()
        });
        let d2 = 0.5 * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t + d1 + 0.03,
            dur: d2,
            hz: b.hz([haw * 1.2, haw, haw * 0.8]),
            attack: 0.03,
            release: 0.1,
            amp,
            form: b.form([700.0, 1200.0, 2500.0]),
            form_end: b.form([650.0, 1100.0, 2400.0]),
            fgain: [1.0, 0.8, 0.35],
            fq: 3.0,
            bright: 0.8,
            breath: 0.3,
            jitter: 0.05,
            rough: 0.5,
            ..Syl::default()
        });
        t += d1 + d2 + 0.1 * b.tk;
    }
}

fn blow(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // A whale's spout, then the breath back in.
    let dur = r.range(1.2, 2.0) * b.tk;
    p.push(Syl {
        src: Src::Hiss,
        at: 0.0,
        dur,
        attack: 0.04,
        release: dur * 0.7,
        form: b.form([300.0, 800.0, 2000.0]),
        form_end: b.form([280.0, 700.0, 1800.0]),
        fgain: [1.0, 0.8, 0.4],
        fq: 1.0,
        ..Syl::default()
    });
    p.push(Syl {
        src: Src::Hiss,
        at: dur + r.range(0.8, 1.2),
        dur: 0.6 * b.tk,
        attack: 0.15,
        release: 0.3,
        amp: 0.4,
        form: b.form([400.0, 1000.0, 2400.0]),
        form_end: b.form([450.0, 1100.0, 2400.0]),
        fgain: [1.0, 0.8, 0.4],
        fq: 1.0,
        ..Syl::default()
    });
}

fn moan(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Long sliding tones, each going its own way: a humpback's song, or
    // the sky rays'.
    let n = 3 + r.next() % 4;
    let mut t = 0.0;
    for _ in 0..n {
        let f = r.range(150.0, 600.0);
        let dur = r.range(1.0, 3.0) * b.tk;
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur,
            hz: b.hz([f, f * r.range(0.7, 1.5), f * r.range(0.6, 1.6)]),
            mid: r.range(0.3, 0.7),
            attack: 0.3,
            release: 0.5,
            vib: [r.range(2.0, 5.0), 0.01],
            bright: 0.4,
            breath: 0.05,
            jitter: 0.0,
            ..Syl::default()
        });
        t += dur + r.range(0.5, 1.5) * b.tk;
    }
}

fn groan(p: &mut Pattern, b: &Build, r: &mut Rng) {
    let n = 1 + (r.unit() < 0.4) as u32;
    let mut t = 0.0;
    for _ in 0..n {
        let f = r.range(85.0, 120.0);
        let dur = r.range(1.2, 2.2) * b.tk;
        p.push(Syl {
            src: Src::Rough,
            at: t,
            dur,
            hz: b.hz([f, f * 1.1, f * 0.8]),
            attack: 0.2,
            release: 0.5,
            vib: [r.range(3.0, 5.0), 0.03],
            form: b.form([400.0, 900.0, 2400.0]),
            form_end: b.form([600.0, 1100.0, 2400.0]),
            fgain: [1.0, 0.7, 0.3],
            fq: 4.0,
            bright: 0.5,
            breath: 0.35,
            jitter: 0.08,
            rough: 0.3,
            ..Syl::default()
        });
        t += dur + r.range(0.5, 1.0) * b.tk;
    }
}

fn flap(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Wings beating hard to get up: a clatter at the wingbeat.
    let dur = r.range(0.8, 1.5) * b.tk;
    p.push(Syl {
        src: Src::Hiss,
        at: 0.0,
        dur,
        attack: 0.0,
        release: dur * 0.6,
        trem: [r.range(8.0, 12.0) / b.tk, 1.0, 3.0],
        form: b.form([700.0, 1800.0, 4000.0]),
        form_end: b.form([700.0, 1800.0, 4000.0]),
        fgain: [1.0, 0.8, 0.5],
        fq: 1.2,
        ..Syl::default()
    });
}

fn splash(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // The slap of the body on the water, then the drops coming back down,
    // each a little bubble ringing up in pitch.
    let dur = r.range(0.2, 0.3) * b.tk;
    p.push(Syl {
        src: Src::Hiss,
        at: 0.0,
        dur,
        attack: 0.0,
        release: dur * 0.8,
        form: b.form([400.0, 1200.0, 3000.0]),
        form_end: b.form([350.0, 1000.0, 2600.0]),
        fgain: [1.0, 0.8, 0.5],
        fq: 0.8,
        ..Syl::default()
    });
    for _ in 0..(4 + r.next() % 6) {
        let f = r.range(800.0, 2000.0);
        let d = r.range(0.02, 0.04);
        p.push(Syl {
            src: Src::Tone,
            at: dur * 0.5 + r.range(0.0, 0.5) * b.tk,
            dur: d,
            hz: b.hz([f, f * 1.4, f * 1.9]),
            attack: 0.002,
            release: d * 0.7,
            amp: r.range(0.25, 0.5),
            bright: 0.1,
            jitter: 0.0,
            ..Syl::default()
        });
    }
}

fn thump(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // A hind foot stamped on the ground, in a quick run.
    let n = 3 + r.next() % 5;
    let every = r.range(0.12, 0.18) * b.tk;
    for i in 0..n {
        let t = i as f32 * every;
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur: 0.07 * b.tk,
            hz: b.hz([140.0, 90.0, 70.0]),
            mid: 0.3,
            attack: 0.002,
            release: 0.05,
            bright: 0.3,
            jitter: 0.0,
            ..Syl::default()
        });
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur: 0.025,
            attack: 0.0,
            release: 0.02,
            amp: 0.3,
            form: b.form([300.0, 800.0, 2000.0]),
            form_end: b.form([300.0, 800.0, 2000.0]),
            fgain: [1.0, 0.6, 0.3],
            fq: 1.0,
            ..Syl::default()
        });
    }
}

fn clash(p: &mut Pattern, b: &Build, r: &mut Rng) {
    // Horn or skull meeting: the crack, and the ring of it. Sparring
    // antlers clatter in a run of smaller ones.
    let n = if r.unit() < 0.5 { 1 } else { 3 + r.next() % 6 };
    let ring = r.range(300.0, 600.0);
    let mut t = 0.0;
    for i in 0..n {
        let amp = if i == 0 { 1.0 } else { r.range(0.4, 0.8) };
        p.push(Syl {
            src: Src::Hiss,
            at: t,
            dur: 0.06 * b.tk,
            attack: 0.0,
            release: 0.05 * b.tk,
            amp,
            form: b.form([600.0, 1400.0, 3000.0]),
            form_end: b.form([600.0, 1400.0, 3000.0]),
            fgain: [1.0, 0.8, 0.4],
            fq: 3.0,
            ..Syl::default()
        });
        p.push(Syl {
            src: Src::Tone,
            at: t,
            dur: 0.3 * b.tk,
            hz: b.hz([ring, ring * 0.98, ring * 0.96]),
            attack: 0.001,
            release: 0.28 * b.tk,
            amp: amp * 0.4,
            bright: 0.5,
            jitter: 0.0,
            ..Syl::default()
        });
        t += r.range(0.08, 0.2) * b.tk;
    }
}

/// Where one syllable's sound has got to.
#[derive(Clone)]
struct SylState {
    begun: bool,
    /// Phase through the current period, 0 to 1.
    p: f32,
    /// This period's length against the pitch (jitter), and loudness
    /// (shimmer); and whether it is an odd period, for the rough voice.
    per: f32,
    shim: f32,
    odd: bool,
    tilt: OnePole,
    f: [Biquad; 3],
    /// Bandpassed breath round the pitch, for a tone.
    nb: Biquad,
    rng: Rng,
}

impl SylState {
    fn new(seed: u32) -> SylState {
        SylState {
            begun: false,
            p: 0.0,
            per: 1.0,
            shim: 1.0,
            odd: false,
            tilt: OnePole::default(),
            f: Default::default(),
            nb: Biquad::default(),
            rng: Rng::new(seed),
        }
    }
}

/// The pitch through a syllable, `u` 0 to 1, eased on a log scale.
fn contour(s: &Syl, u: f32) -> f32 {
    let ease = |x: f32| 0.5 - 0.5 * (PI * x.clamp(0.0, 1.0)).cos();
    let (a, b, x) = if u < s.mid {
        (s.hz[0], s.hz[1], u / s.mid.max(1e-3))
    } else {
        (s.hz[1], s.hz[2], (u - s.mid) / (1.0 - s.mid).max(1e-3))
    };
    let (la, lb) = (a.max(1.0).ln(), b.max(1.0).ln());
    (la + (lb - la) * ease(x)).exp()
}

/// A band-limited saw's correction at a discontinuity (polyBLEP).
#[inline]
fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        2.0 * x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + 2.0 * x + 1.0
    } else {
        0.0
    }
}

/// A call under way.
struct Voice {
    cry: Cry,
    pat: Pattern,
    st: [SylState; SYLS],
    t: f32,
    wait: usize,
    gain: f32,
    gain_to: f32,
    lr: (f32, f32),
    air: [OnePole; 2],
    pitch: f32,
    key: u32,
    seed: u32,
}

impl Voice {
    /// Add one block of the call, mono, into `buf`.
    fn render(&mut self, buf: &mut [f32], sr: f32) {
        let n = buf.len();
        let dt = 1.0 / sr;
        let t0 = self.t;
        let t1 = t0 + n as f32 * dt;
        let pitch = self.pitch;
        for i in 0..self.pat.n {
            let s = self.pat.syl[i];
            if s.at + s.dur <= t0 || s.at >= t1 {
                continue;
            }
            let (before, rest) = self.st.split_at_mut(i);
            let st = &mut rest[0];
            if !st.begun {
                st.begun = true;
                if s.tie && i > 0 {
                    let seed = st.rng.next();
                    *st = before[i - 1].clone();
                    st.rng = Rng::new(seed);
                }
            }
            // Filters once a block, at its middle.
            let um = (((t0 + t1) * 0.5 - s.at) / s.dur).clamp(0.0, 1.0);
            let f0 = contour(&s, um) * pitch;
            let formed = s.fgain.iter().any(|g| *g > 0.0);
            if formed {
                for j in 0..3 {
                    let c = s.form[j] + (s.form_end[j] - s.form[j]) * um;
                    st.f[j].bandpass(c * pitch.sqrt(), s.fq, sr);
                }
            }
            st.nb.bandpass(f0, 4.0, sr);
            st.tilt.set((f0 * (1.5 + 8.0 * s.bright)).min(sr * 0.45), sr);
            for (j, out) in buf.iter_mut().enumerate() {
                let ts = t0 + j as f32 * dt - s.at;
                if ts < 0.0 || ts >= s.dur {
                    continue;
                }
                let u = ts / s.dur;
                let mut f = contour(&s, u) * pitch;
                if s.vib[1] > 0.0 {
                    f *= 1.0 + s.vib[1] * (TAU * s.vib[0] * ts).sin();
                }
                let mut am = 1.0;
                if s.trem[1] > 0.0 {
                    let q = (s.trem[0] * ts).fract();
                    am = 1.0 - s.trem[1] + s.trem[1] * (PI * q).sin().powf(s.trem[2]);
                    f *= 1.0 - s.chirp * q;
                }
                let mut env = 1.0;
                if s.attack > 0.0 && ts < s.attack {
                    env = (0.5 * PI * ts / s.attack).sin().powi(2);
                }
                let left = s.dur - ts;
                if s.release > 0.0 && left < s.release {
                    env *= (0.5 * PI * left / s.release).sin().powi(2);
                }
                let x = match s.src {
                    Src::Tone => {
                        st.p = (st.p + f * dt).fract();
                        let ph = TAU * st.p;
                        let h = s.bright;
                        let tone = (ph.sin() + h * (0.5 * (2.0 * ph).sin() + 0.25 * (3.0 * ph).sin())) / (1.0 + 0.75 * h);
                        tone + s.breath * 3.0 * st.nb.run(st.rng.bi())
                    }
                    Src::Voiced | Src::Rough => {
                        let rough = if s.src == Src::Rough { s.rough } else { 0.0 };
                        let step = (f * dt * st.per).min(0.5);
                        st.p += step;
                        if st.p >= 1.0 {
                            st.p -= 1.0;
                            st.odd = !st.odd;
                            let long = if st.odd { 1.0 } else { 1.0 - 0.12 * rough };
                            st.per = (1.0 + s.jitter * st.rng.bi()) * long;
                            st.shim = 1.0 + 3.0 * s.jitter * st.rng.bi();
                        }
                        let saw = 2.0 * st.p - 1.0 - blep(st.p, step);
                        let weak = if st.odd { 1.0 - 0.6 * rough } else { 1.0 };
                        let mut x = saw * st.shim * weak;
                        x = st.tilt.lp(x);
                        x += rough * 0.8 * st.rng.bi() * (1.0 - st.p).powi(4);
                        x + s.breath * 0.5 * st.rng.bi()
                    }
                    Src::Hiss => st.rng.bi(),
                };
                let y = if formed { (0..3).map(|k| s.fgain[k] * st.f[k].run(x)).sum::<f32>() } else { x };
                *out += y * env * am * s.amp;
            }
        }
        self.t = t1;
    }

    fn done(&self) -> bool {
        self.t > self.pat.len + 0.05
    }
}

/// The pool of calls under way.
pub struct Calls {
    voices: Vec<Voice>,
}

/// Calls at once, at most; the quietest goes to make room.
pub const VOICES: usize = 16;

impl Default for Calls {
    fn default() -> Self {
        Calls::new()
    }
}

impl Calls {
    pub fn new() -> Calls {
        Calls {
            voices: Vec::with_capacity(VOICES),
        }
    }

    /// Start a call, or, when the same caller's same call (`key` and
    /// `seed`) is already sounding, move it: a call is posted again each
    /// frame from where its caller now is.
    pub fn start(&mut self, s: &super::fx::Shot, cry: Cry, size: f32, seed: u32, sr: f32) {
        let lr = pan(s.pan);
        if s.key != 0 {
            if let Some(v) = self.voices.iter_mut().find(|v| v.key == s.key && v.seed == seed && v.cry == cry) {
                v.gain_to = s.gain;
                v.lr = lr;
                for a in &mut v.air {
                    a.set(s.cut, sr);
                }
                return;
            }
        }
        if self.voices.len() >= VOICES {
            let (i, _) = self.voices.iter().enumerate().min_by(|a, b| a.1.gain.total_cmp(&b.1.gain)).unwrap();
            self.voices.swap_remove(i);
        }
        let mut air = [OnePole::default(), OnePole::default()];
        for a in &mut air {
            a.set(s.cut, sr);
        }
        self.voices.push(Voice {
            cry,
            pat: cry.pattern(size, seed),
            st: std::array::from_fn(|k| SylState::new(seed.wrapping_add(k as u32 * 7919))),
            t: 0.0,
            wait: (s.delay * sr) as usize,
            gain: s.gain,
            gain_to: s.gain,
            lr,
            air,
            pitch: s.pitch,
            key: s.key,
            seed,
        });
    }

    /// Run every call for one block, adding into `l`, `r` and the reverb
    /// send `send`.
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32], send: &mut [f32], sr: f32) {
        let n = l.len();
        let mut buf = [0.0f32; super::BLOCK];
        self.voices.retain_mut(|v| {
            let mut first = 0;
            if v.wait >= n {
                v.wait -= n;
                return true;
            } else if v.wait > 0 {
                first = v.wait;
                v.wait = 0;
            }
            let b = &mut buf[..n - first];
            b.fill(0.0);
            v.render(b, sr);
            let g0 = v.gain;
            v.gain += (v.gain_to - v.gain) * 0.2;
            let span = b.len().max(1) as f32;
            for (i, x) in b.iter().enumerate() {
                let g = g0 + (v.gain - g0) * (i as f32 / span);
                let y = x * g;
                let o = first + i;
                l[o] += v.air[0].lp(y * v.lr.0);
                r[o] += v.air[1].lp(y * v.lr.1);
                send[o] += y;
            }
            !v.done()
        });
    }
}

/// Render one call dry, mono, at gain 1: for tests and the call bake.
pub fn render_dry(cry: Cry, size: f32, seed: u32, sr: f32) -> Vec<f32> {
    let pat = cry.pattern(size, seed);
    let mut v = Voice {
        cry,
        pat,
        st: std::array::from_fn(|k| SylState::new(seed.wrapping_add(k as u32 * 7919))),
        t: 0.0,
        wait: 0,
        gain: 1.0,
        gain_to: 1.0,
        lr: (1.0, 1.0),
        air: Default::default(),
        pitch: 1.0,
        key: 0,
        seed,
    };
    let total = ((pat.len + 0.1) * sr) as usize;
    let mut out = vec![0.0f32; total.div_ceil(super::BLOCK) * super::BLOCK];
    for chunk in out.chunks_mut(super::BLOCK) {
        v.render(chunk, sr);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(x: &[f32]) -> f32 {
        20.0 * x.iter().fold(0.0f32, |a, v| a.max(v.abs())).max(1e-9).log10()
    }

    #[test]
    fn every_cry_peaks_alike() {
        let mut bad = Vec::new();
        for c in CRIES {
            for seed in [1, 2, 3] {
                let x = render_dry(c, 1.0, seed, 48_000.0);
                let p = peak(&x);
                println!("{:8} seed {seed}  peak {p:6.1} dB  {:.1} s", c.name(), x.len() as f32 / 48_000.0);
                if !(-12.0..=0.0).contains(&p) {
                    bad.push(format!("{} {seed}: {p:.1}", c.name()));
                }
            }
        }
        assert!(bad.is_empty(), "out of range: {bad:?}");
    }

    #[test]
    fn same_seed_same_call() {
        let a = render_dry(Cry::Song, 1.0, 9, 48_000.0);
        let b = render_dry(Cry::Song, 1.0, 9, 48_000.0);
        let c = render_dry(Cry::Song, 1.0, 10, 48_000.0);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn bigger_is_lower() {
        let small = Cry::Moo.pattern(0.7, 4);
        let big = Cry::Moo.pattern(1.5, 4);
        assert!(big.syl[0].hz[0] < small.syl[0].hz[0]);
        assert!(big.len > small.len);
    }

    #[test]
    fn a_distant_call_is_duller_and_quieter() {
        assert!(air_cut(500.0) < air_cut(20.0));
        assert!(gain_of(Cry::Roar.spl(), 0.0) > gain_of(Cry::Song.spl(), 0.0));
    }
}
