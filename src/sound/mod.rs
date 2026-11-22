//! Sound. Everything is synthesised while it plays; nothing is recorded.
//!
//! The split is the renderer's. `Game` works out what the ear gets this
//! moment as a row of numbers (`Heard`, made by `bed.rs`), the window hands
//! it over once a frame with the listener's facing, and the engine here plays
//! it. The engine knows nothing about voxels, biomes or weather; `bed.rs` is
//! the one file that does.
//!
//! The mix is three buses -- ambience, effects (the guns, the player's own
//! noises, the animals) and music -- each with its own level in the
//! settings, through one master.
//!
//! Two rules hold for everything that runs in the audio callback: nothing
//! allocates, locks or blocks there (the moment's numbers cross over through
//! a `try_lock`, skipped when the game holds it), and every number glides
//! (`dsp::Glide`), so nothing the game does in a frame steps in the output.

pub mod bake;
pub mod bed;
pub mod dsp;
pub mod fx;
pub mod voice;
pub mod watch;

pub use fx::{Kind, Noise, Pick};

use dsp::{Glide, OnePole, Room, Wander};
use fx::{Fx, Shot};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use voice::{Brook, Falls, Mix, Motor, Now, Rain, Rustle, Rustling, Surf, Under, Wind};

/// What the ground under the listener is, for what a raindrop sounds like
/// landing on it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Ground {
    /// Earth, sand, snow, mud: a dull pat.
    #[default]
    Soft,
    Grass,
    /// Leaves overhead or underfoot.
    Leaves,
    /// Rock, paving, a roof: bright and short.
    Stone,
    /// Standing in it: drops plink.
    Water,
}

impl Ground {
    pub fn name(self) -> &'static str {
        match self {
            Ground::Soft => "soft",
            Ground::Grass => "grass",
            Ground::Leaves => "leaves",
            Ground::Stone => "stone",
            Ground::Water => "water",
        }
    }
}

/// Something heard from a direction: running water, a waterfall, the sea.
/// `dir` is where it is in the world (x, z), unit length, or zero when it is
/// all round; `spread` is how much of the horizon it fills, 0 (a point) to 1
/// (everywhere). A river 50 m off is narrow; standing in one, it is wide.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Emitter {
    pub gain: f32,
    pub dir: [f32; 2],
    pub spread: f32,
}

/// What the ear gets this moment, as numbers. Every voice gain runs 0
/// (silent) to 1 (as loud as that voice gets). A place with nothing in it,
/// on a still day, with no water near, is every field 0, and the engine then
/// produces exact silence.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heard {
    /// The air itself, low and broad, for how exposed the place is.
    pub wind: f32,
    /// The day's wind, 0 (still) to 1 (a gale): what drives the gusts, and
    /// through them every rustle.
    pub gust: f32,
    /// The wail of the wind over an edge: a ridge, a summit, the air high up.
    pub whistle: f32,
    /// Leaves near the head, and grass about the feet.
    pub leaves: f32,
    pub grass: f32,
    /// Falling rain, and how much of the sky is shut out over the listener
    /// (0 open, 1 roofed), which takes the near drops away and dulls the rest.
    pub rain: f32,
    pub shelter: f32,
    pub ground: Ground,
    /// Water dripping off the leaves overhead, which goes on for a while
    /// after the rain stops.
    pub drip: f32,
    /// Running water, and how fast it runs, 0 (still) to 1 (rapids).
    pub water: Emitter,
    pub flow: f32,
    /// The nearest waterfall; how big a drop it is (0 to about 1.5); and the
    /// spray close to its foot.
    pub falls: Emitter,
    pub falls_size: f32,
    pub spray: f32,
    /// Waves on a shore.
    pub surf: Emitter,
    /// The nearest wave breaking where the surf is drawn (world/surf.rs):
    /// its phase (0 as the crest passes), period in seconds and size (0.2
    /// to 1). Period 0 where the world has no surf, and the voice keeps its
    /// own time.
    pub surf_wave: [f32; 3],
    /// The ear is under water.
    pub under: f32,
    /// How far the top of the whole mix is rolled off, 0 to 1: fog, snow
    /// lying, the hush of a closed place.
    pub muffle: f32,
    /// Reverb send, 0 to 1, and its character, 0 (a wood: short and dark)
    /// to 1 (a canyon or a cave).
    pub room: f32,
    pub room_size: f32,
    /// The car being driven: how loud its engine is (0 off), the firing rate
    /// in Hz, how open the throttle is, and how hard the tyres slide, 0 to 1.
    pub motor: f32,
    pub motor_hz: f32,
    pub motor_load: f32,
    pub scrub: f32,
}

/// The player's levels, kept with the other settings (`menu::Prefs`). Each
/// runs 0 to 100 in the SETTINGS list.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    /// The master switch: off lets go of the device altogether.
    pub on: bool,
    pub volume: u8,
    pub ambience: u8,
    /// The guns, the player's own noises and the animals.
    pub effects: u8,
    pub music: u8,
}

impl Default for Levels {
    fn default() -> Self {
        Levels {
            on: true,
            volume: 70,
            ambience: 100,
            effects: 100,
            music: 60,
        }
    }
}

/// The buses the mix is made of, in the order `Levels` lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Ambience,
    Effects,
    #[allow(dead_code)] // the score, when there is one
    Music,
}

impl Levels {
    /// A bus's gain with the master in it, 0 to 1. Levels are heard on a
    /// curve, so 50 sounds like half rather than nearly full.
    pub fn gain(&self, b: Bus) -> f32 {
        if !self.on {
            return 0.0;
        }
        let bus = match b {
            Bus::Ambience => self.ambience,
            Bus::Effects => self.effects,
            Bus::Music => self.music,
        };
        let c = |v: u8| (v.min(100) as f32 / 100.0).powi(2);
        c(self.volume) * c(bus)
    }
}

/// What the game hands the engine: the moment, the listener's facing (x, z
/// of the camera's forward, unit length), the levels, and how much of the
/// ambience to let through (1 playing, less behind a menu, 0 with no world).
#[derive(Clone, Copy, Debug)]
pub struct Cue {
    pub heard: Heard,
    pub facing: [f32; 2],
    pub levels: Levels,
    pub duck: f32,
}

impl Default for Cue {
    fn default() -> Self {
        Cue {
            heard: Heard::default(),
            facing: [0.0, -1.0],
            levels: Levels::default(),
            duck: 0.0,
        }
    }
}

/// The engine against the effects: about -22 dBFS at full throttle.
const MOTOR_LEVEL: f32 = 0.18;

/// One-shots waiting for the callback, at most.
const SHOTS_KEPT: usize = 128;

/// Samples a block: everything that is not a sample (glides, filter
/// coefficients, event rates) is worked out once a block.
pub const BLOCK: usize = 64;

/// How loud the ambience is at full strength, before the levels: a voice at
/// gain 1 comes out about -30 dBFS, so the bed sits near the edge of
/// hearing at the default volume. Measured by `tests::every_voice_is_about_as_loud`.
const AMBIENCE_LEVEL: f32 = 0.05;

/// How loud the effects are at full strength, before the levels: a sidearm
/// round at the ear peaks about -10 dBFS (`tests::a_round_at_the_ear`).
const EFFECTS_LEVEL: f32 = 0.45;

/// Sounds posted and not yet taken (`emit`, `take_noises`). Nobody listens
/// in a headless match or a tournament, so posting is a no-op until a
/// `Sound` exists; and a game that never takes them keeps only the latest.
static NOISES: Mutex<Vec<Noise>> = Mutex::new(Vec::new());
static LISTENING: AtomicBool = AtomicBool::new(false);
const NOISES_KEPT: usize = 256;

/// Post a sound: anything in the game may, from anywhere, and it costs a
/// lock and a push. It is placed against the listener and played the next
/// time the window hands sound over.
pub fn emit(n: Noise) {
    if !LISTENING.load(Ordering::Relaxed) {
        return;
    }
    if let Ok(mut q) = NOISES.lock() {
        if q.len() >= NOISES_KEPT {
            q.remove(0);
        }
        q.push(n);
    }
}

/// Start taking posted sounds without a device (`--sound-bake`).
pub fn listen() {
    LISTENING.store(true, Ordering::Relaxed);
}

/// Take every sound posted since the last call.
pub fn take_noises() -> Vec<Noise> {
    NOISES.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}

/// The glides, one per number the engine reads, and how fast each moves.
struct Glides {
    wind: Glide,
    gust: Glide,
    whistle: Glide,
    leaves: Glide,
    grass: Glide,
    rain: Glide,
    shelter: Glide,
    drip: Glide,
    water: Glide,
    water_dir: [Glide; 2],
    water_spread: Glide,
    flow: Glide,
    falls: Glide,
    falls_dir: [Glide; 2],
    falls_spread: Glide,
    falls_size: Glide,
    spray: Glide,
    surf: Glide,
    surf_dir: [Glide; 2],
    surf_spread: Glide,
    under: Glide,
    muffle: Glide,
    room: Glide,
    room_size: Glide,
    facing: [Glide; 2],
    ambience: Glide,
    effects: Glide,
    motor: Glide,
    motor_hz: Glide,
    motor_load: Glide,
    scrub: Glide,
}

impl Glides {
    fn new(br: f32) -> Glides {
        let g = |s| Glide::new(0.0, s, br);
        Glides {
            wind: g(0.8),
            gust: g(2.0),
            whistle: g(1.2),
            leaves: g(0.8),
            grass: g(0.8),
            rain: g(2.5),
            shelter: g(0.5),
            drip: g(3.0),
            water: g(0.8),
            water_dir: [g(0.4), g(0.4)],
            water_spread: g(0.6),
            flow: g(1.5),
            falls: g(0.8),
            falls_dir: [g(0.4), g(0.4)],
            falls_spread: g(0.6),
            falls_size: g(1.5),
            spray: g(0.5),
            surf: g(1.5),
            surf_dir: [g(0.5), g(0.5)],
            surf_spread: g(0.8),
            under: g(0.12),
            muffle: g(1.0),
            room: g(1.5),
            room_size: g(2.0),
            facing: [Glide::new(0.0, 0.04, br), Glide::new(-1.0, 0.04, br)],
            ambience: g(0.6),
            effects: Glide::new(0.0, 0.05, br),
            motor: g(0.15),
            motor_hz: Glide::new(30.0, 0.06, br),
            motor_load: g(0.08),
            scrub: g(0.1),
        }
    }

    fn aim(&mut self, c: &Cue) {
        let h = &c.heard;
        self.wind.to = h.wind;
        self.gust.to = h.gust;
        self.whistle.to = h.whistle;
        self.leaves.to = h.leaves;
        self.grass.to = h.grass;
        self.rain.to = h.rain;
        self.shelter.to = h.shelter;
        self.drip.to = h.drip;
        let em = |g: &mut Glide, d: &mut [Glide; 2], s: &mut Glide, e: &Emitter| {
            g.to = e.gain;
            // A source that has gone silent keeps its last direction while
            // it fades, rather than swinging to the middle.
            if e.gain > 0.0 {
                d[0].to = e.dir[0];
                d[1].to = e.dir[1];
                s.to = e.spread;
            }
        };
        em(&mut self.water, &mut self.water_dir, &mut self.water_spread, &h.water);
        self.flow.to = h.flow;
        em(&mut self.falls, &mut self.falls_dir, &mut self.falls_spread, &h.falls);
        self.falls_size.to = h.falls_size;
        self.spray.to = h.spray;
        em(&mut self.surf, &mut self.surf_dir, &mut self.surf_spread, &h.surf);
        self.under.to = h.under;
        self.muffle.to = h.muffle;
        self.room.to = h.room;
        self.room_size.to = h.room_size;
        self.motor.to = h.motor;
        if h.motor > 0.0 {
            self.motor_hz.to = h.motor_hz;
        }
        self.motor_load.to = h.motor_load;
        self.scrub.to = h.scrub;
        self.facing[0].to = c.facing[0];
        self.facing[1].to = c.facing[1];
        self.ambience.to = c.levels.gain(Bus::Ambience) * c.duck.clamp(0.0, 1.0);
        self.effects.to = c.levels.gain(Bus::Effects) * c.duck.clamp(0.0, 1.0);
    }
}

/// Where an emitter sits in the stereo field for a listener facing `f`:
/// (pan centre, width), as `voice::Now` carries it. Turning to face a river
/// brings it to the middle; one all round stays all round.
fn place(dir: [f32; 2], spread: f32, f: [f32; 2]) -> (f32, f32) {
    let len = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
    let flen = (f[0] * f[0] + f[1] * f[1]).sqrt().max(1e-4);
    if len < 0.05 {
        return (0.0, 1.0);
    }
    // Right of the facing, in x and z: forward x up.
    let right = [-f[1] / flen, f[0] / flen];
    let side = (dir[0] * right[0] + dir[1] * right[1]) / len;
    let spread = (spread + (1.0 - len.min(1.0))).clamp(0.0, 1.0);
    (side * (1.0 - spread), spread)
}

/// The mixer: every voice, the room and the way out. It knows nothing about
/// a device; `Sound` drives it from the callback and `bake.rs` drives it
/// into a file.
pub struct Engine {
    sr: f32,
    cue: Cue,
    g: Glides,
    gust: [Wander; 3],
    whine: Wander,
    wind: Wind,
    leaves: Rustle,
    grass: Rustle,
    rain: Rain,
    brook: Brook,
    falls: Falls,
    surf: Surf,
    under: Under,
    motor: Motor,
    room: Room,
    fx: Fx,
    muffle: [[OnePole; 2]; 2],
    l: [f32; BLOCK],
    r: [f32; BLOCK],
    /// Samples of the current block not yet handed out.
    at: usize,
}

impl Engine {
    pub fn new(sr: f32) -> Engine {
        let br = sr / BLOCK as f32;
        Engine {
            sr,
            cue: Cue::default(),
            g: Glides::new(br),
            gust: [Wander::new(11, 0.05, br), Wander::new(12, 0.17, br), Wander::new(13, 0.5, br)],
            whine: Wander::new(14, 0.07, br),
            wind: Wind::new(100),
            leaves: Rustle::new(200, Rustling::Leaves),
            grass: Rustle::new(300, Rustling::Grass),
            rain: Rain::new(400),
            brook: Brook::new(500),
            falls: Falls::new(600),
            surf: Surf::new(700),
            under: Under::new(800),
            motor: Motor::new(900),
            room: Room::new(sr),
            fx: Fx::new(),
            muffle: Default::default(),
            l: [0.0; BLOCK],
            r: [0.0; BLOCK],
            at: BLOCK,
        }
    }

    pub fn set(&mut self, c: Cue) {
        self.cue = c;
    }

    /// Start a one-shot (or keep a held one up).
    pub fn start(&mut self, s: Shot) {
        self.fx.start(s, self.sr);
    }

    /// One block of the mix into `self.l` and `self.r`.
    fn block(&mut self) {
        self.g.aim(&self.cue);
        let g = &mut self.g;
        let facing = [g.facing[0].step(), g.facing[1].step()];
        let gust: f32 = self.gust.iter_mut().zip([0.5, 0.3, 0.2]).map(|(w, k)| w.step() * k).sum();
        let day = g.gust.step();
        let under = g.under.step();
        // Above the surface fades as the ear goes under it.
        let up = 1.0 - under;
        let dir = |d: &mut [Glide; 2]| [d[0].step(), d[1].step()];
        let wd = dir(&mut g.water_dir);
        let fd = dir(&mut g.falls_dir);
        let sd = dir(&mut g.surf_dir);
        let n = Now {
            blow: day * (0.35 + 1.3 * gust.powf(1.5)),
            wind: g.wind.step() * up,
            whistle: g.whistle.step() * up,
            whine: self.whine.step(),
            leaves: g.leaves.step() * up,
            grass: g.grass.step() * up,
            rain: g.rain.step() * up,
            shelter: g.shelter.step(),
            ground: self.cue.heard.ground,
            drip: g.drip.step() * up,
            water: g.water.step(),
            water_pan: place(wd, g.water_spread.step(), facing),
            flow: g.flow.step(),
            falls: g.falls.step(),
            falls_pan: place(fd, g.falls_spread.step(), facing),
            falls_size: g.falls_size.step(),
            spray: g.spray.step() * up,
            surf: g.surf.step(),
            surf_pan: place(sd, g.surf_spread.step(), facing),
            surf_wave: self.cue.heard.surf_wave,
            under,
        };
        let muffle = g.muffle.step();
        let room = g.room.step();
        let size = g.room_size.step();
        let amb = g.ambience.step();
        let eff = g.effects.step();
        self.l = [0.0; BLOCK];
        self.r = [0.0; BLOCK];
        let mut m = Mix {
            l: &mut self.l,
            r: &mut self.r,
            sr: self.sr,
        };
        self.wind.run(&n, &mut m);
        self.leaves.run(&n, &mut m);
        self.grass.run(&n, &mut m);
        self.rain.run(&n, &mut m);
        self.brook.run(&n, &mut m);
        self.falls.run(&n, &mut m);
        self.surf.run(&n, &mut m);
        self.under.run(&n, &mut m);
        // The room, as a send off the whole ambience.
        self.room.shape(size, self.sr);
        // Fog and snow dull the top; going under takes nearly all of it.
        let mut cut = 16_000.0 + (2_500.0 - 16_000.0) * muffle;
        cut += (650.0 - cut) * under;
        for ch in &mut self.muffle {
            for p in ch.iter_mut() {
                p.set(cut, self.sr);
            }
        }
        let (mut fl, mut fr, mut fs) = ([0.0f32; BLOCK], [0.0f32; BLOCK], [0.0f32; BLOCK]);
        self.fx.run(&mut fl, &mut fr, &mut fs, self.sr);
        // The car's engine rides the effects bus, under the guns.
        let (mut ml, mut mr) = ([0.0f32; BLOCK], [0.0f32; BLOCK]);
        let (mg, mh, mload, ms) = (g.motor.step(), g.motor_hz.step(), g.motor_load.step(), g.scrub.step());
        self.motor.run(mg, mh, mload, ms, &mut Mix { l: &mut ml, r: &mut mr, sr: self.sr });
        for i in 0..BLOCK {
            fl[i] += ml[i] * MOTOR_LEVEL;
            fr[i] += mr[i] * MOTOR_LEVEL;
        }
        let amb = AMBIENCE_LEVEL * amb;
        let eff = EFFECTS_LEVEL * eff;
        // Outdoors a shot still wants a little of the land answering it.
        let shot_room = 0.25 + 0.75 * room;
        for i in 0..BLOCK {
            let into = (self.l[i] + self.r[i]) * 0.5 * room * amb + fs[i] * shot_room * eff;
            let (wl, wr) = self.room.run(into);
            let mut l = self.l[i] * amb + fl[i] * eff + wl * 0.7;
            let mut r = self.r[i] * amb + fr[i] * eff + wr * 0.7;
            l = self.muffle[0][0].lp(l);
            l = self.muffle[0][1].lp(l);
            r = self.muffle[1][0].lp(r);
            r = self.muffle[1][1].lp(r);
            self.l[i] = limit(l);
            self.r[i] = limit(r);
        }
    }

    /// Fill interleaved output with `channels` a frame: left and right in
    /// the first two, nothing in any others, both mixed for a mono device.
    pub fn fill(&mut self, out: &mut [f32], channels: usize) {
        let ch = channels.max(1);
        for frame in out.chunks_mut(ch) {
            if self.at >= BLOCK {
                self.block();
                self.at = 0;
            }
            let (l, r) = (self.l[self.at], self.r[self.at]);
            self.at += 1;
            if ch == 1 {
                frame[0] = (l + r) * 0.5;
            } else {
                frame[0] = l;
                frame[1] = r;
                for s in frame.iter_mut().skip(2) {
                    *s = 0.0;
                }
            }
        }
    }
}

/// A soft ceiling: untouched below half scale, eased toward full scale above
/// it, so a pile-up never clips into the speakers.
#[inline]
fn limit(x: f32) -> f32 {
    let a = x.abs();
    if a <= 0.5 {
        x
    } else {
        x.signum() * (0.5 + 0.5 * ((a - 0.5) * 2.0).tanh())
    }
}

/// The sound device: a cpal output stream running the engine on the
/// device's own thread, and the slot the game writes the moment into.
pub struct Sound {
    stream: Option<cpal::Stream>,
    slot: Arc<Mutex<Cue>>,
    /// One-shots placed and waiting for the callback to start them.
    shots: Arc<Mutex<Vec<Shot>>>,
    rng: dsp::Rng,
    broken: Arc<AtomicBool>,
    retry: Option<Instant>,
    levels: Levels,
    pub device: String,
    pub rate: u32,
}

impl Sound {
    /// Open the default output device, or leave the game silent (with a line
    /// on stderr) when there is none: a missing sound card is not a reason
    /// not to play.
    pub fn new(levels: Levels) -> Sound {
        let mut s = Sound {
            stream: None,
            slot: Arc::new(Mutex::new(Cue { levels, ..Cue::default() })),
            shots: Arc::new(Mutex::new(Vec::with_capacity(SHOTS_KEPT))),
            rng: dsp::Rng::new(7),
            broken: Arc::new(AtomicBool::new(false)),
            retry: None,
            levels,
            device: String::new(),
            rate: 0,
        };
        if levels.on {
            s.open();
        }
        LISTENING.store(true, Ordering::Relaxed);
        s
    }

    /// Place the sounds posted this frame against the listener and queue
    /// them for the callback. Each round's pitch wanders a few per cent, so
    /// a held trigger is not one sample on a loop.
    pub fn play(&mut self, noises: &[Noise], eye: glam::Vec3, facing: [f32; 2]) {
        if noises.is_empty() || self.stream.is_none() {
            return;
        }
        if let Ok(mut q) = self.shots.lock() {
            for n in noises {
                let pitch = 1.0 + (self.rng.unit() - 0.5) * 0.06;
                if let Some(s) = fx::place(n, eye, facing, pitch) {
                    if q.len() < SHOTS_KEPT {
                        q.push(s);
                    }
                }
            }
        }
    }

    fn open(&mut self) {
        match self.start() {
            Ok(()) => self.broken.store(false, Ordering::Relaxed),
            Err(e) => {
                eprintln!("sound: {e}");
                self.stream = None;
                self.retry = Some(Instant::now());
            }
        }
    }

    fn start(&mut self) -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no output device")?;
        let cfg = dev.default_output_config().map_err(|e| e.to_string())?;
        self.device = dev.name().unwrap_or_default();
        self.rate = cfg.sample_rate().0;
        let channels = cfg.channels() as usize;
        let config: cpal::StreamConfig = cfg.config();
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => self.build::<f32>(&dev, &config, channels),
            cpal::SampleFormat::I16 => self.build::<i16>(&dev, &config, channels),
            cpal::SampleFormat::U16 => self.build::<u16>(&dev, &config, channels),
            f => return Err(format!("the device wants {f:?} samples, which this does not write")),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build<T: cpal::SizedSample + cpal::FromSample<f32>>(&self, dev: &cpal::Device, config: &cpal::StreamConfig, channels: usize) -> Result<cpal::Stream, String> {
        use cpal::traits::DeviceTrait;
        let mut engine = Engine::new(config.sample_rate.0 as f32);
        let slot = self.slot.clone();
        let shots = self.shots.clone();
        let broken = self.broken.clone();
        // The engine writes f32; a device that wants something else gets it
        // converted from a scratch buffer sized once, here.
        let mut scratch = vec![0.0f32; 1 << 15];
        dev.build_output_stream(
            config,
            move |out: &mut [T], _: &cpal::OutputCallbackInfo| {
                if let Ok(c) = slot.try_lock() {
                    engine.set(*c);
                }
                if let Ok(mut q) = shots.try_lock() {
                    for s in q.drain(..) {
                        engine.start(s);
                    }
                }
                for chunk in out.chunks_mut(scratch.len()) {
                    let buf = &mut scratch[..chunk.len()];
                    engine.fill(buf, channels);
                    for (o, s) in chunk.iter_mut().zip(buf.iter()) {
                        *o = T::from_sample(*s);
                    }
                }
            },
            move |e| {
                eprintln!("sound: {e}");
                broken.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| e.to_string())
    }

    /// Hand over this frame's moment. Also where a device that went away
    /// (headphones pulled out) is opened again, at most every two seconds.
    pub fn update(&mut self, heard: Heard, facing: [f32; 2], duck: f32) {
        if let Ok(mut c) = self.slot.lock() {
            *c = Cue {
                heard,
                facing,
                levels: self.levels,
                duck,
            };
        }
        if !self.levels.on {
            return;
        }
        if self.broken.load(Ordering::Relaxed) && self.stream.is_some() {
            self.stream = None;
            self.retry = Some(Instant::now());
        }
        if self.stream.is_none() && self.retry.is_some_and(|t| t.elapsed().as_secs_f32() > 2.0) {
            self.open();
        }
    }

    /// The settings changed. Off lets go of the device; on takes it back.
    pub fn set_levels(&mut self, l: Levels) {
        self.levels = l;
        if !l.on {
            self.stream = None;
            self.retry = None;
        } else if self.stream.is_none() {
            self.open();
        }
        if let Ok(mut c) = self.slot.lock() {
            c.levels = l;
        }
    }

    pub fn playing(&self) -> bool {
        self.stream.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(c: Cue, secs: f32) -> Vec<f32> {
        let sr = 48_000.0;
        let mut e = Engine::new(sr);
        e.set(c);
        let mut out = vec![0.0; (sr * secs) as usize * 2];
        e.fill(&mut out, 2);
        out
    }

    fn rms(v: &[f32]) -> f32 {
        (v.iter().map(|x| x * x).sum::<f32>() / v.len().max(1) as f32).sqrt()
    }

    fn db(x: f32) -> f32 {
        20.0 * x.max(1e-9).log10()
    }

    fn full(h: Heard) -> Cue {
        Cue {
            heard: h,
            facing: [0.0, -1.0],
            levels: Levels {
                on: true,
                volume: 100,
                ambience: 100,
                ..Levels::default()
            },
            duck: 1.0,
        }
    }

    /// Nothing in the place, nothing in the weather: not one sample off zero.
    #[test]
    fn an_empty_moment_is_exact_silence() {
        let out = render(full(Heard::default()), 2.0);
        assert!(out.iter().all(|x| *x == 0.0), "a quiet place made a sound");
        // And sound off is silence whatever the place.
        let mut c = full(Heard {
            wind: 1.0,
            gust: 1.0,
            rain: 1.0,
            ..Heard::default()
        });
        c.levels.on = false;
        let out = render(c, 2.0);
        assert!(out.iter().skip(1000).all(|x| *x == 0.0));
    }

    /// Every voice at full strength comes out within a few dB of the others,
    /// and none of them anywhere near clipping. This is what AMBIENCE_LEVEL
    /// is tuned against.
    #[test]
    fn every_voice_is_about_as_loud() {
        let e = Emitter {
            gain: 1.0,
            dir: [1.0, 0.0],
            spread: 0.3,
        };
        let voices: Vec<(&str, Heard)> = vec![
            (
                "wind",
                Heard {
                    wind: 1.0,
                    gust: 0.7,
                    ..Heard::default()
                },
            ),
            (
                "whistle",
                Heard {
                    whistle: 1.0,
                    gust: 0.9,
                    ..Heard::default()
                },
            ),
            (
                "leaves",
                Heard {
                    leaves: 1.0,
                    gust: 0.7,
                    ..Heard::default()
                },
            ),
            (
                "grass",
                Heard {
                    grass: 1.0,
                    gust: 0.7,
                    ..Heard::default()
                },
            ),
            ("rain", Heard { rain: 1.0, ..Heard::default() }),
            ("drip", Heard { drip: 1.0, ..Heard::default() }),
            (
                "brook",
                Heard {
                    water: e,
                    flow: 0.8,
                    ..Heard::default()
                },
            ),
            (
                "river",
                Heard {
                    water: e,
                    flow: 0.2,
                    ..Heard::default()
                },
            ),
            (
                "falls",
                Heard {
                    falls: e,
                    falls_size: 1.0,
                    spray: 1.0,
                    ..Heard::default()
                },
            ),
            ("surf", Heard { surf: e, ..Heard::default() }),
            ("under", Heard { under: 1.0, ..Heard::default() }),
        ];
        let mut levels = vec![];
        for (name, h) in voices {
            let out = render(full(h), 12.0);
            // The first seconds are the glides arriving.
            let tail = &out[out.len() / 2..];
            let peak = tail.iter().fold(0.0f32, |a, x| a.max(x.abs()));
            assert!(tail.iter().all(|x| x.is_finite()), "{name} made a NaN");
            let r = db(rms(tail));
            println!("{name:8} rms {r:6.1} dBFS  peak {:6.1} dBFS", db(peak));
            assert!(peak < 0.5, "{name} peaks at {peak}");
            levels.push((name, r));
        }
        for (name, r) in &levels {
            // Drips are a few a second: judged by their peaks, not their
            // average, so they are left out of the line-up.
            if *name == "drip" {
                continue;
            }
            assert!(*r > -38.0 && *r < -26.0, "{name} at {r:.1} dBFS is out of line with the rest");
        }
    }

    /// Everything at once, at its worst: still finite and never clipping.
    #[test]
    fn everything_at_once_stays_in_bounds() {
        let e = Emitter {
            gain: 1.0,
            dir: [0.0, 1.0],
            spread: 0.0,
        };
        let h = Heard {
            wind: 1.0,
            gust: 1.0,
            whistle: 1.0,
            leaves: 1.0,
            grass: 1.0,
            rain: 1.0,
            drip: 1.0,
            water: e,
            flow: 1.0,
            falls: e,
            falls_size: 1.5,
            spray: 1.0,
            surf: e,
            room: 1.0,
            room_size: 1.0,
            ..Heard::default()
        };
        let out = render(full(h), 10.0);
        assert!(out.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
    }

    /// A river off to the right is louder in the right ear; turn round and
    /// it is in the left.
    #[test]
    fn a_river_is_where_it_is() {
        let h = Heard {
            water: Emitter {
                gain: 1.0,
                dir: [1.0, 0.0],
                spread: 0.1,
            },
            flow: 0.6,
            ..Heard::default()
        };
        // Facing -z, +x is to the right.
        let ears = |facing: [f32; 2]| {
            let mut c = full(h);
            c.facing = facing;
            let out = render(c, 6.0);
            let tail = &out[out.len() / 2..];
            let l: Vec<f32> = tail.iter().step_by(2).copied().collect();
            let r: Vec<f32> = tail.iter().skip(1).step_by(2).copied().collect();
            (rms(&l), rms(&r))
        };
        let (l, r) = ears([0.0, -1.0]);
        assert!(r > l * 2.0, "facing north, a river to the east: l {l} r {r}");
        let (l, r) = ears([0.0, 1.0]);
        assert!(l > r * 2.0, "facing south: l {l} r {r}");
    }

    /// Every effect, heard at the ear at full volume: loud enough to hear,
    /// never clipping, and a sidearm round about where EFFECTS_LEVEL says.
    #[test]
    fn a_round_at_the_ear() {
        use fx::{Kind as K, Pick, Report as R};
        let kinds = [
            K::Shot(R::Pistol),
            K::Shot(R::Minigun),
            K::Shot(R::Sniper),
            K::Shot(R::Swing),
            K::Shot(R::Rocket),
            K::Shot(R::Lob),
            K::Shot(R::Scatter),
            K::Shot(R::Disc),
            K::Shot(R::Goo),
            K::Shot(R::Slug),
            K::Beam(R::Pulse),
            K::Beam(R::Bore),
            K::Rush,
            K::Impact(Ground::Stone),
            K::Impact(Ground::Soft),
            K::Ricochet,
            K::Whizz { from: glam::Vec3::ZERO, crack: false },
            K::Whizz { from: glam::Vec3::ZERO, crack: true },
            K::Bounce,
            K::Burst(5.0),
            K::Splat(3.0),
            K::Sparks,
            K::Hit { head: true },
            K::Kill,
            K::Hurt(0.3),
            K::Death,
            K::Clank,
            K::Shutdown,
            K::Pickup(Pick::Weapon),
            K::Pickup(Pick::Ammo),
            K::Switch,
            K::ReloadIn,
            K::Step(Ground::Grass),
            K::Step(Ground::Stone),
            K::Land(10.0),
            K::Pad,
            K::Teleport,
            K::FlagCaptured(true),
            K::Dig(Ground::Stone),
            K::Place,
        ];
        for k in kinds {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            // Let the effects level arrive before the sound does.
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let n = fx::Noise::ear(k, 1.0);
            let s = fx::place(&n, glam::Vec3::ZERO, [0.0, -1.0], 1.0).unwrap();
            e.start(s);
            let mut out = vec![0.0; (sr * 3.0) as usize * 2];
            // A held beam is kept up for half a second, then let go.
            for (i, chunk) in out.chunks_mut(960).enumerate() {
                if matches!(k, K::Beam(_) | K::Rush) && i < 50 {
                    e.start(s);
                }
                e.fill(chunk, 2);
            }
            let peak = out.iter().fold(0.0f32, |a, x| a.max(x.abs()));
            println!("{k:?}: peak {:.1} dBFS", db(peak));
            assert!(out.iter().all(|x| x.is_finite()), "{k:?} made a NaN");
            assert!(peak > 0.01, "{k:?} is all but silent: {peak}");
            assert!(peak < 0.75, "{k:?} peaks at {peak}");
            // Everything is over within three seconds, the biggest burst's
            // rumble and the room's tail included.
            let tail = &out[out.len() - 4800..];
            assert!(tail.iter().all(|x| x.abs() < 0.01), "{k:?} is still sounding after three seconds");
        }
    }

    /// Grass in the wind, alone and then under the wind itself, a light
    /// breeze and then a stiff one: `$LURELIA_FX_WAVS/grass.wav`.
    #[test]
    #[ignore]
    fn bake_the_grass() {
        let Ok(dir) = std::env::var("LURELIA_FX_WAVS") else { return };
        let mut out = vec![];
        for (wind, gust) in [(0.0, 0.35), (0.0, 0.9), (0.6, 0.35), (0.6, 0.9)] {
            let o = render(
                full(Heard {
                    grass: 1.0,
                    wind,
                    gust,
                    ..Heard::default()
                }),
                8.0,
            );
            // The glides arriving are not the sound.
            out.extend_from_slice(&o[o.len() / 4..]);
        }
        bake::write_wav(&std::path::PathBuf::from(dir).join("grass.wav"), 48_000, &out).unwrap();
    }

    /// Running water at each of `$LURELIA_CREEK`'s "gain,flow;gain,flow"
    /// (what `bed::probe` prints), six seconds each, to
    /// `$LURELIA_FX_WAVS/creek.wav`.
    #[test]
    #[ignore]
    fn bake_the_creek() {
        let (Ok(dir), Ok(at)) = (std::env::var("LURELIA_FX_WAVS"), std::env::var("LURELIA_CREEK")) else {
            return;
        };
        let mut out = vec![];
        for p in at.split(';') {
            let v: Vec<f32> = p.split(',').map(|x| x.trim().parse().unwrap()).collect();
            let water = Emitter {
                gain: v[0],
                dir: [1.0, 0.0],
                spread: 0.5,
            };
            let o = render(
                full(Heard {
                    water,
                    flow: v[1],
                    ..Heard::default()
                }),
                8.0,
            );
            out.extend_from_slice(&o[o.len() / 4..]);
        }
        bake::write_wav(&std::path::PathBuf::from(dir).join("creek.wav"), 48_000, &out).unwrap();
    }

    /// Each gun's report at the ear, written to `$LURELIA_FX_WAVS/<gun>.wav`
    /// to be listened to or measured: `cargo test bake_the_guns -- --ignored`.
    #[test]
    #[ignore]
    fn bake_the_guns() {
        use fx::{Kind as K, Report as R};
        let Ok(dir) = std::env::var("LURELIA_FX_WAVS") else { return };
        let dir = std::path::PathBuf::from(dir);
        let guns = [
            ("pistol", K::Shot(R::Pistol)),
            ("minigun", K::Shot(R::Minigun)),
            ("sniper", K::Shot(R::Sniper)),
            ("scatter", K::Shot(R::Scatter)),
            ("slug", K::Shot(R::Slug)),
            ("rocket", K::Shot(R::Rocket)),
            ("burst", K::Burst(5.0)),
            ("lob", K::Shot(R::Lob)),
        ];
        for (name, k) in guns {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let mut out = vec![0.0; (sr * 2.0) as usize * 2];
            // Twice, half a second apart, the way a gun is heard.
            for (i, chunk) in out.chunks_mut(960).enumerate() {
                if i == 0 || i == 50 {
                    e.start(fx::place(&fx::Noise::ear(k, fx::loudness_of(k)), glam::Vec3::ZERO, [0.0, -1.0], 1.0).unwrap());
                }
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join(format!("{name}.wav")), 48_000, &out).unwrap();
        }
        // Two seconds of the minigun held down, a round each time its
        // 1100 rpm comes round on a 60 Hz step, as the game fires it.
        {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let mut out = vec![0.0; (sr * 2.5) as usize * 2];
            let (mut due, gap) = (0.0f32, 60.0 / 1100.0);
            let mut rng = dsp::Rng::new(5);
            for (i, chunk) in out.chunks_mut(800 * 2).enumerate() {
                let t = i as f32 / 60.0;
                if t < 2.0 && t >= due {
                    due += gap;
                    let pitch = 1.0 + (rng.unit() - 0.5) * 0.06;
                    e.start(fx::place(&fx::Noise::ear(K::Shot(R::Minigun), fx::loudness_of(K::Shot(R::Minigun))), glam::Vec3::ZERO, [0.0, -1.0], pitch).unwrap());
                }
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join("minigun_burst.wav"), 48_000, &out).unwrap();
        }
        // Rounds from a gun 30 m ahead going past: pistol rounds 1.5 m to
        // the left, a minigun stream 3 m to the right, then a sniper round
        // a metre overhead, each heard as it would be (its report from
        // the muzzle, late by the distance, and the whizz at once).
        {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let mut out = vec![0.0; (sr * 4.0) as usize * 2];
            let muzzle = glam::Vec3::new(0.0, 0.0, -30.0);
            let ear = glam::Vec3::ZERO;
            let put = |e: &mut Engine, n: fx::Noise| {
                if let Some(s) = fx::place(&n, ear, [0.0, -1.0], 1.0) {
                    e.start(s);
                }
            };
            for (i, chunk) in out.chunks_mut(800 * 2).enumerate() {
                let (r, to) = match i {
                    0 | 25 | 50 => (R::Pistol, glam::Vec3::new(-1.5, 0.0, 20.0)),
                    90..=150 if i % 3 == 0 => (R::Minigun, glam::Vec3::new(3.0, 0.0, 20.0)),
                    190 => (R::Sniper, glam::Vec3::new(0.0, 1.0, 20.0)),
                    _ => {
                        e.fill(chunk, 2);
                        continue;
                    }
                };
                put(&mut e, fx::Noise::at(K::Shot(r), muzzle, fx::loudness_of(K::Shot(r))));
                put(&mut e, fx::Noise::at(K::Whizz { from: muzzle, crack: r == R::Sniper }, to, if r == R::Sniper { 1.2 } else { 0.8 }));
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join("whizz.wav"), 48_000, &out).unwrap();
        }
        // Walking at 3 m/s, a step every half second, on each ground in
        // turn: four steps of each, at the feet 1.6 m under the ear.
        {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let grounds = [Ground::Stone, Ground::Grass, Ground::Leaves, Ground::Soft, Ground::Water];
            let mut out = vec![0.0; (sr * 10.5) as usize * 2];
            let feet = glam::Vec3::new(0.0, -1.6, 0.0);
            for (i, chunk) in out.chunks_mut(960).enumerate() {
                if i % 50 == 0 && i / 200 < grounds.len() {
                    let n = fx::Noise::at(K::Step(grounds[i / 200]), feet, 0.3 * 0.9);
                    e.start(fx::place(&n, glam::Vec3::ZERO, [0.0, -1.0], 1.0).unwrap());
                }
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join("steps.wav"), 48_000, &out).unwrap();
        }
        // The bore held for a second and a half.
        {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let mut out = vec![0.0; (sr * 2.0) as usize * 2];
            let s = fx::place(
                &fx::Noise {
                    key: 1,
                    ..fx::Noise::ear(K::Beam(R::Bore), 0.45)
                },
                glam::Vec3::ZERO,
                [0.0, -1.0],
                1.0,
            )
            .unwrap();
            for (i, chunk) in out.chunks_mut(960).enumerate() {
                if i < 150 {
                    e.start(s);
                }
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join("bore.wav"), 48_000, &out).unwrap();
        }
        // A rocket fired from the ear down a 60 m hall, and one flying past
        // from left to right three metres out, each bursting where it ends.
        let flights: [(&str, glam::Vec3, glam::Vec3); 2] = [
            ("rocket_away", glam::Vec3::ZERO, glam::Vec3::new(0.0, 0.0, -37.4)),
            ("rocket_past", glam::Vec3::new(-50.0, 0.0, -3.0), glam::Vec3::new(37.4, 0.0, 0.0)),
        ];
        for (name, from, vel) in flights {
            let sr = 48_000.0;
            let mut e = Engine::new(sr);
            e.set(full(Heard::default()));
            let mut warm = vec![0.0; 4800 * 2];
            e.fill(&mut warm, 2);
            let mut out = vec![0.0; (sr * 3.5) as usize * 2];
            let ear = glam::Vec3::ZERO;
            let put = |e: &mut Engine, n: fx::Noise| {
                if let Some(s) = fx::place(&n, ear, [0.0, -1.0], 1.0) {
                    e.start(s);
                }
            };
            for (i, chunk) in out.chunks_mut(960).enumerate() {
                let t = i as f32 * 0.01;
                let at = from + vel * t;
                if i == 0 {
                    put(&mut e, fx::Noise::at(K::Shot(R::Rocket), from, fx::loudness_of(K::Shot(R::Rocket))));
                }
                if i < 160 {
                    put(&mut e, fx::Noise::rush(at, vel, ear, 0));
                } else if i == 160 {
                    put(&mut e, fx::Noise::at(K::Burst(5.0), at, fx::loudness_of(K::Burst(5.0))));
                }
                e.fill(chunk, 2);
            }
            bake::write_wav(&dir.join(format!("{name}.wav")), 48_000, &out).unwrap();
        }
    }

    /// A round passing a metre to the right whizzes, on the right; one fired
    /// by the listener, one that stopped short of them and one that went
    /// wide do not.
    #[test]
    fn a_round_going_past_whizzes() {
        let ear = glam::Vec3::ZERO;
        let round = |from: glam::Vec3, to: glam::Vec3| fx::Noise::at(fx::Kind::Whizz { from, crack: false }, to, 0.8);
        let by = fx::place(&round(glam::Vec3::new(1.0, 0.0, -40.0), glam::Vec3::new(1.0, 0.0, 20.0)), ear, [0.0, -1.0], 1.0).expect("no whizz");
        assert!(by.pan > 0.3, "{by:?}");
        let mine = round(glam::Vec3::new(0.3, -0.2, -0.3), glam::Vec3::new(0.0, 0.0, -50.0));
        assert!(fx::place(&mine, ear, [0.0, -1.0], 1.0).is_none(), "my own round whizzed");
        let short = round(glam::Vec3::new(1.0, 0.0, -40.0), glam::Vec3::new(1.0, 0.0, -10.0));
        assert!(fx::place(&short, ear, [0.0, -1.0], 1.0).is_none(), "a round that stopped short whizzed");
        let wide = round(glam::Vec3::new(9.0, 0.0, -40.0), glam::Vec3::new(9.0, 0.0, 20.0));
        assert!(fx::place(&wide, ear, [0.0, -1.0], 1.0).is_none(), "a wide round whizzed");
    }

    /// Far off, a shot is quieter, later and duller.
    #[test]
    fn distance_is_quieter_later_and_duller() {
        let n = fx::Noise::at(fx::Kind::Shot(fx::Report::Pistol), glam::Vec3::new(0.0, 0.0, -100.0), 1.0);
        let near = fx::place(
            &fx::Noise {
                at: Some(glam::Vec3::new(0.0, 0.0, -2.0)),
                ..n
            },
            glam::Vec3::ZERO,
            [0.0, -1.0],
            1.0,
        )
        .unwrap();
        let far = fx::place(&n, glam::Vec3::ZERO, [0.0, -1.0], 1.0).unwrap();
        assert!(far.gain < near.gain * 0.1 && far.delay > 0.25 && far.cut < near.cut * 0.7, "{near:?} {far:?}");
        // Something a long way off and quiet is not worth a voice.
        assert!(fx::place(&fx::Noise { gain: 0.05, ..n }, glam::Vec3::ZERO, [0.0, -1.0], 1.0).is_none());
        // To the right is to the right.
        let right = fx::place(
            &fx::Noise {
                at: Some(glam::Vec3::new(20.0, 0.0, 0.0)),
                ..n
            },
            glam::Vec3::ZERO,
            [0.0, -1.0],
            1.0,
        )
        .unwrap();
        assert!(right.pan > 0.8, "{right:?}");
    }

    #[test]
    fn levels_are_a_curve_with_a_switch() {
        let l = Levels::default();
        assert!((l.gain(Bus::Ambience) - 0.49).abs() < 1e-3);
        assert_eq!(Levels { on: false, ..l }.gain(Bus::Ambience), 0.0);
        assert_eq!(Levels { volume: 0, ..l }.gain(Bus::Ambience), 0.0);
    }
}
