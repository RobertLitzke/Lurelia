//! Effects: every sound that happens once -- a gunshot, a round striking a
//! wall, a burst, a footstep, a pickup -- on the effects bus.
//!
//! Anything in the game may post one with `sound::emit(Noise)`: a world
//! position (or none, for a sound at the ear) and what kind of thing it was.
//! The window takes the posted noises once a frame, places each against the
//! listener (`Sound::play`: how far, which side, how much air it came
//! through, how late it arrives at 343 m/s) and hands them to the engine,
//! which plays them from a fixed pool of voices.
//!
//! A voice is a `Recipe`: up to eight layers, each a noise or an oscillator
//! with a pitch that glides, a filter whose cutoff glides, an attack and an
//! exponential decay, the sum driven through a soft clipper if the recipe
//! asks. A gunshot is made of noise, not tones: a sine falling in pitch is a
//! drum, and the guns sounded like drums until they lost theirs. It is a
//! pulse (the muzzle blast's own pressure wave, `Src::Pulse`), a crack
//! (white noise highpassed, two or three milliseconds, the supersonic snap),
//! a blast (broadband noise, tens of milliseconds), a boom (noise lowpassed
//! near 150 Hz, the bass that has no pitch) and a tail (the report coming
//! back off the land), all driven so the peaks flatten into grit. The guns
//! differ in the numbers. Nothing is recorded.

use super::dsp::{pan, Biquad, OnePole, Rng};
use super::Ground;
use crate::library::{FireDef, FireKind, SplashLook};
use glam::Vec3;

/// What a gun sounds like, worked out from what its fire mode does
/// (`Report::of`), so a new weapon in the library sounds like something the
/// day it is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Report {
    /// A light hitscan gun: the sidearm.
    Pistol,
    /// Hitscan at a cadence too fast to hear one round from the next.
    Minigun,
    /// One heavy hitscan round.
    Sniper,
    /// A close swing with a burst on the end of it: the digger.
    Swing,
    /// A rocket leaving the tube.
    Rocket,
    /// A lob: a grenade or a cluster leaving a short barrel.
    Lob,
    /// A handful of pellets: the scrap gun, the minigun's flak.
    Scatter,
    /// A spinning disc that comes off walls.
    Disc,
    /// A glob of sludge.
    Goo,
    /// A heavy slug: the bore's thrown spoil.
    Slug,
    /// Held beams: a pulse of light, and the bore's cutting head.
    Pulse,
    Bore,
}

impl Report {
    pub fn of(f: &FireDef) -> Report {
        let goo = f.goo_r > 0.0 || f.splash.as_ref().is_some_and(|s| s.look == SplashLook::Goo);
        let splash = f.splash.as_ref().map_or(0.0, |s| s.radius);
        match f.kind {
            FireKind::Beam if f.bore_r > 0.0 => Report::Bore,
            FireKind::Beam => Report::Pulse,
            FireKind::Hitscan if f.range < 6.0 => Report::Swing,
            FireKind::Hitscan if f.rpm >= 1000.0 => Report::Minigun,
            FireKind::Hitscan if f.damage * f.pellets.max(1) as f32 >= 40.0 => Report::Sniper,
            FireKind::Hitscan if f.pellets > 1 => Report::Scatter,
            FireKind::Hitscan => Report::Pistol,
            FireKind::Shell if goo => Report::Goo,
            FireKind::Shell if splash >= 4.0 => Report::Rocket,
            FireKind::Shell if f.bounces >= 2 => Report::Disc,
            FireKind::Shell if f.pellets > 1 => Report::Scatter,
            FireKind::Shell if f.gravity >= 8.0 => Report::Lob,
            FireKind::Shell => Report::Slug,
        }
    }
}

/// A pickup, by what was in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Health,
    Armour,
    Ammo,
    Weapon,
}

/// Something that happened, for the effects bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Shot(Report),
    /// A beam being held; posted every step it is held, and the voice runs
    /// until the posts stop (`Noise::key` says whose).
    Beam(Report),
    /// A round striking the world, by what it struck.
    Impact(Ground),
    /// A rocket in flight: held like a beam for as long as it flies, posted
    /// from where it is each frame (`Noise::key` says which).
    Rush,
    /// A round passing close by: its line from the muzzle to where it
    /// stopped (`Noise::at` is the far end). `place` finds where it came
    /// nearest the ear. `crack` is a supersonic round, which snaps as well.
    Whizz { from: Vec3, crack: bool },
    /// A round whining off something hard.
    Ricochet,
    /// A shell coming off a wall.
    Bounce,
    /// A burst, by its radius in metres.
    Burst(f32),
    /// A burst of sludge.
    Splat(f32),
    Sparks,
    /// The player's round landed: on the body, or in the head.
    Hit { head: bool },
    /// The player killed someone.
    Kill,
    /// The player was hurt, by how much of their health (0 to 1).
    Hurt(f32),
    /// The player died.
    Death,
    /// A bot hit, and a bot down.
    Clank,
    Shutdown,
    Pickup(Pick),
    /// A bot picked something up, off to one side.
    Grab,
    /// The weapon changed, a reload began, a reload ended.
    Switch,
    ReloadOut,
    ReloadIn,
    Step(Ground),
    Jump,
    /// Landing, by how fast the fall was, m/s.
    Land(f32),
    /// A jump pad's launch and a teleporter's crossing.
    Pad,
    Teleport,
    /// The player coming back in a match.
    Respawn,
    /// A flag taken, lost, returned or brought home: ours (true) or theirs.
    FlagTaken(bool),
    FlagDropped,
    FlagReturned,
    FlagCaptured(bool),
    /// Digging and placing by hand.
    Dig(Ground),
    Place,
}

/// A sound posted by the game. `at` is where it happened, or None for a
/// sound at the ear (the HUD's tick when a round lands). `gain` is how loud
/// it is at a couple of metres, 1 being a sidearm round; `key` tells one
/// held beam from another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    pub kind: Kind,
    pub at: Option<Vec3>,
    pub gain: f32,
    pub key: u32,
    /// Pitch against the recipe's, 1 as made: a moving source's Doppler
    /// shift.
    pub pitch: f32,
}

impl Noise {
    pub fn at(kind: Kind, at: Vec3, gain: f32) -> Noise {
        Noise { kind, at: Some(at), gain, key: 0, pitch: 1.0 }
    }

    pub fn ear(kind: Kind, gain: f32) -> Noise {
        Noise { kind, at: None, gain, key: 0, pitch: 1.0 }
    }

    /// A round fired from `at` by fire mode `f`.
    pub fn fire(at: Vec3, f: &FireDef) -> Noise {
        let r = Report::of(f);
        Noise::at(Kind::Shot(r), at, loudness(r))
    }

    /// A beam held at `at` by fighter `by`.
    pub fn beam(at: Vec3, f: &FireDef, by: usize) -> Noise {
        let r = Report::of(f);
        // The bore drones for as long as it is held, so it sits well under
        // a shot that is over in a moment: 5 dB under the pulse gun.
        let gain = if r == Report::Bore { 0.45 } else { 0.8 };
        Noise { kind: Kind::Beam(r), at: Some(at), gain, key: by as u32 + 1, pitch: 1.0 }
    }

    /// A hitscan round from `from` to `to`, for the whizz if it passes
    /// close to whoever is listening. Only a heavy round (the sniper's) is
    /// taken to be faster than sound.
    pub fn past(from: Vec3, to: Vec3, f: &FireDef) -> Noise {
        if f.kind != FireKind::Hitscan {
            return Noise::ear(Kind::Whizz { from, crack: false }, 0.0);
        }
        let crack = Report::of(f) == Report::Sniper;
        Noise::at(Kind::Whizz { from, crack }, to, if crack { 1.2 } else { 0.8 })
    }

    /// A rocket at `at` flying at `vel`, heard from `ear`; `key` tells it
    /// from the other rockets in the air.
    pub fn rush(at: Vec3, vel: Vec3, ear: Vec3, key: u32) -> Noise {
        // Coming on, higher; going away, lower: c / (c + speed away).
        let away = vel.dot((at - ear).normalize_or_zero());
        let pitch = (343.0 / (343.0 + away)).clamp(0.6, 1.6);
        Noise { kind: Kind::Rush, at: Some(at), gain: 1.0, key: 1000 + key, pitch }
    }
}

/// How loud a kind is posted by the game, for measuring one alone.
#[cfg(test)]
pub fn loudness_of(k: Kind) -> f32 {
    match k {
        Kind::Shot(r) => loudness(r),
        Kind::Burst(r) => 2.0 * (r / 3.0).clamp(0.4, 2.0).sqrt(),
        _ => 1.0,
    }
}

/// How loud each gun is, relative to the sidearm.
fn loudness(r: Report) -> f32 {
    match r {
        Report::Pistol => 1.0,
        Report::Minigun => 0.8,
        Report::Sniper => 1.8,
        Report::Swing => 0.7,
        Report::Rocket => 1.4,
        Report::Lob => 0.8,
        Report::Scatter => 1.4,
        Report::Disc => 0.9,
        Report::Goo => 0.8,
        Report::Slug => 1.2,
        Report::Pulse | Report::Bore => 0.8,
    }
}

#[derive(Clone, Copy, Debug)]
enum Src {
    Noise,
    Sine,
    Saw,
    Square,
    /// A muzzle blast's pressure pulse (the Friedlander waveform): a jump,
    /// a fall through zero, a shallow negative phase. `hz` is one over the
    /// positive phase, a millisecond or two. A burst of noise is heard as
    /// "chh"; this is heard as a pop.
    Pulse,
}

#[derive(Clone, Copy, Debug)]
enum Filt {
    Off,
    Lp,
    Bp,
    Hp,
}

/// One layer of a recipe. Times in seconds, pitches and cutoffs in Hz; a
/// pitch or cutoff glides from its first value toward its second with time
/// constant `glide` (0 holds the first).
#[derive(Clone, Copy, Debug)]
struct Layer {
    src: Src,
    hz: [f32; 2],
    filt: Filt,
    cut: [f32; 2],
    q: f32,
    glide: f32,
    amp: f32,
    at: f32,
    attack: f32,
    decay: f32,
}

const L: Layer = Layer { src: Src::Noise, hz: [0.0, 0.0], filt: Filt::Off, cut: [0.0, 0.0], q: 0.7, glide: 0.0, amp: 1.0, at: 0.0, attack: 0.001, decay: 0.05 };

/// Noise through a bandpass: a crack, a click, a hiss.
const fn band(hz: f32, q: f32, amp: f32, decay: f32) -> Layer {
    Layer { filt: Filt::Bp, cut: [hz, hz], q, amp, decay, ..L }
}

/// Noise through a lowpass: a body, a rumble, a tail.
const fn low(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { filt: Filt::Lp, cut: [hz, hz], amp, decay, ..L }
}

/// Noise through a highpass: sparks, spray, a hiss.
const fn high(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { filt: Filt::Hp, cut: [hz, hz], amp, decay, ..L }
}

/// A sine falling from `a` to `b`: a thump, a chime.
const fn tone(a: f32, b: f32, glide: f32, amp: f32, decay: f32) -> Layer {
    Layer { src: Src::Sine, hz: [a, b], glide, amp, decay, ..L }
}

/// The crack: white noise highpassed, over in a few milliseconds, with no
/// attack to speak of.
const fn crack(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { attack: 0.0001, ..high(hz, amp, decay) }
}

/// A muzzle blast's pulse (`Src::Pulse`), positive phase `ms` long,
/// highpassed so the shot does not push the speaker cone out and leave it
/// there. Its envelope only ends the layer: the pulse shapes itself.
const fn pulse(ms: f32, amp: f32) -> Layer {
    Layer { src: Src::Pulse, hz: [1000.0 / ms, 1000.0 / ms], filt: Filt::Hp, cut: [150.0, 150.0], attack: 0.0, decay: ms * 0.004, amp, ..L }
}

/// The blast: broadband noise, a little longer than the crack.
const fn blast(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { attack: 0.0003, ..low(hz, amp, decay) }
}

/// The boom: noise lowpassed, the bass of a shot without the pitch of a
/// drum. A resonant edge would ring at the cutoff, and a ringing low note
/// is a drum again, so the q stays at 1. Lowpassed noise near 150 Hz keeps
/// little of its power, so the amps are large.
const fn boom(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { q: 1.0, attack: 0.002, ..low(hz, amp, decay) }
}

/// The tail: the report coming back off the land, a little behind.
const fn tail(hz: f32, amp: f32, decay: f32) -> Layer {
    Layer { at: 0.015, attack: 0.03, ..low(hz, amp, decay) }
}

const fn later(l: Layer, at: f32) -> Layer {
    Layer { at, ..l }
}

const fn swept(l: Layer, to: f32, glide: f32) -> Layer {
    Layer { cut: [l.cut[0], to], glide, ..l }
}

const fn slow(l: Layer, attack: f32) -> Layer {
    Layer { attack, ..l }
}

const fn from(l: Layer, src: Src) -> Layer {
    Layer { src, ..l }
}

/// The most layers a recipe has.
const LAYERS: usize = 8;

/// A recipe: up to eight layers, the reverb send it asks for, and how hard
/// its sum is driven into the soft clipper (0 is not at all).
struct Recipe {
    layers: [Layer; LAYERS],
    n: usize,
    send: f32,
    drive: f32,
}

impl Recipe {
    fn layers(&self) -> &[Layer] {
        &self.layers[..self.n]
    }
}

fn recipe(k: Kind) -> Recipe {
    use Kind::*;
    let g = |ls: &[Layer], send: f32, drive: f32| {
        let mut layers = [L; LAYERS];
        layers[..ls.len()].copy_from_slice(ls);
        Recipe { layers, n: ls.len(), send, drive }
    };
    let r = |ls: &[Layer], send: f32| g(ls, send, 0.0);
    match k {
        // Guns: crack, blast, boom, tail (see the module notes), no tones.
        Shot(Report::Pistol) => g(&[pulse(1.2, 1.5), crack(1700.0, 1.6, 0.003), blast(3800.0, 1.0, 0.03), boom(220.0, 2.4, 0.055), tail(780.0, 0.25, 0.2)], 0.6, 1.6),
        // Rat-a-tat: at eighteen rounds a second each one must be over
        // before the next, or they smear into a roar. A pulse for the pop,
        // a short crack and body, a boom cut short, and the action's
        // metallic tick just behind; little room, so the gaps stay clear.
        Shot(Report::Minigun) => g(
            &[pulse(1.2, 1.4), crack(2400.0, 0.8, 0.0015), blast(3500.0, 0.7, 0.012), boom(220.0, 2.2, 0.018), later(band(3800.0, 6.0, 0.3, 0.01), 0.02)],
            0.2,
            1.6,
        ),
        // The sniper's round is supersonic: a whip crack, its echo off the
        // ground a few milliseconds behind, a ring of it above, and the
        // biggest boom and longest tail of the hitscan guns.
        Shot(Report::Sniper) => g(
            &[
                pulse(1.4, 1.6),
                crack(2600.0, 2.2, 0.0025),
                later(crack(3200.0, 1.2, 0.002), 0.007),
                band(5200.0, 1.1, 0.9, 0.012),
                blast(3500.0, 1.1, 0.06),
                boom(150.0, 6.0, 0.17),
                tail(700.0, 0.55, 0.9),
            ],
            0.9,
            2.2,
        ),
        Shot(Report::Swing) => r(&[swept(slow(band(500.0, 1.2, 0.8, 0.07), 0.04), 1800.0, 0.06), later(low(400.0, 0.9, 0.06), 0.06), later(tone(120.0, 50.0, 0.04, 0.8, 0.06), 0.06)], 0.3),
        // A whoomp: the motor lighting in the tube, noise whose lowpass
        // opens upward from a low roar, over a short boom. Nothing above a
        // couple of kHz, which is static rather than rocket. `Rush` carries
        // it on after the rocket has gone.
        Shot(Report::Rocket) => g(
            &[
                pulse(3.0, 0.8),
                boom(140.0, 3.0, 0.12),
                swept(slow(low(180.0, 2.4, 0.35), 0.035), 900.0, 0.12),
                slow(band(600.0, 0.8, 0.9, 0.3), 0.04),
                later(slow(low(400.0, 1.0, 0.5), 0.06), 0.03),
                blast(1800.0, 0.5, 0.03),
            ],
            0.7,
            1.2,
        ),
        // A rocket's motor in flight: a dark roar, held. Its decay is how
        // fast it falls away once the rocket is gone.
        Rush => g(&[low(380.0, 3.5, 0.1), band(900.0, 0.8, 1.0, 0.1), low(1800.0, 0.3, 0.1)], 0.5, 1.2),
        Shot(Report::Lob) => g(&[pulse(2.5, 1.2), crack(1500.0, 0.5, 0.003), band(320.0, 3.0, 1.8, 0.06), boom(160.0, 3.5, 0.08), blast(2000.0, 0.6, 0.03)], 0.4, 1.4),
        Shot(Report::Scatter) => g(&[pulse(1.8, 1.8), crack(1500.0, 1.4, 0.004), blast(3000.0, 1.4, 0.07), boom(130.0, 6.5, 0.16), tail(600.0, 0.5, 0.5)], 0.8, 2.0),
        Shot(Report::Disc) => r(&[from(tone(900.0, 1500.0, 0.05, 0.6, 0.08), Src::Saw), band(4200.0, 2.0, 0.5, 0.04), tone(160.0, 80.0, 0.03, 0.5, 0.04)], 0.4),
        Shot(Report::Goo) => r(&[band(480.0, 3.0, 1.0, 0.1), tone(320.0, 110.0, 0.06, 0.7, 0.12), later(band(900.0, 4.0, 0.4, 0.06), 0.05)], 0.3),
        Shot(Report::Slug) => g(&[pulse(1.6, 1.6), crack(1800.0, 1.0, 0.003), blast(2600.0, 1.0, 0.05), boom(150.0, 5.5, 0.13), tail(500.0, 0.4, 0.4)], 0.6, 1.8),
        // Beams are held: their decay is how fast they fall away once let go.
        // The bore's cutting head: a sawtooth drone at 46 Hz, a grinding
        // rumble under it and a little grit. 55 Hz was too thin and 38 Hz
        // too low.
        Beam(Report::Bore) | Shot(Report::Bore) => g(
            &[from(tone(46.0, 46.0, 0.0, 0.75, 0.12), Src::Saw), low(180.0, 1.8, 0.12), band(260.0, 1.8, 0.8, 0.12), band(2000.0, 2.5, 0.2, 0.12)],
            0.4,
            1.3,
        ),
        Beam(_) | Shot(Report::Pulse) => r(&[from(tone(220.0, 220.0, 0.0, 0.45, 0.1), Src::Saw), tone(440.0, 440.0, 0.0, 0.35, 0.1), band(3400.0, 4.0, 0.2, 0.1)], 0.4),
        Impact(Ground::Stone) => r(&[band(3600.0, 1.4, 0.9, 0.012), band(1200.0, 1.0, 0.5, 0.02)], 0.5),
        Impact(Ground::Water) => r(&[band(1400.0, 2.0, 0.7, 0.03), tone(900.0, 1700.0, 0.03, 0.4, 0.04)], 0.3),
        Impact(Ground::Leaves) => r(&[band(2500.0, 0.8, 0.6, 0.03), low(800.0, 0.4, 0.03)], 0.3),
        Impact(_) => r(&[low(900.0, 0.9, 0.03), band(2000.0, 1.0, 0.3, 0.01)], 0.3),
        // A round going past: narrow noise falling in pitch as it goes (the
        // Doppler sweep), rising fast as it comes. A supersonic round
        // snaps first.
        Whizz { crack: false, .. } => r(&[swept(slow(band(3200.0, 4.0, 1.0, 0.07), 0.025), 1100.0, 0.08), swept(slow(band(6000.0, 3.0, 0.4, 0.05), 0.02), 2500.0, 0.06)], 0.2),
        Whizz { crack: true, .. } => r(
            &[pulse(0.3, 1.2), crack(3000.0, 1.6, 0.002), band(5200.0, 1.2, 0.6, 0.01), swept(slow(band(2800.0, 3.0, 0.8, 0.08), 0.015), 900.0, 0.09)],
            0.4,
        ),
        Ricochet => r(&[tone(2900.0, 1300.0, 0.18, 0.35, 0.2)], 0.6),
        Bounce => r(&[tone(1800.0, 1750.0, 0.0, 0.4, 0.05), tone(2750.0, 2700.0, 0.0, 0.25, 0.04), band(3000.0, 2.0, 0.3, 0.01)], 0.4),
        // An explosion: a sharp front, a deep boom, a whoomp of air whose
        // lowpass closes from bright to dark, a crunch of debris, and a
        // long rumble rolling back off the land.
        Burst(_) => g(
            &[
                pulse(6.0, 1.5),
                crack(1200.0, 0.8, 0.006),
                boom(100.0, 8.0, 0.3),
                swept(low(2400.0, 1.4, 0.45), 250.0, 0.35),
                band(1100.0, 0.8, 0.7, 0.08),
                later(band(2600.0, 1.2, 0.25, 0.15), 0.04),
                later(slow(low(300.0, 1.2, 0.45), 0.06), 0.05),
            ],
            1.0,
            1.6,
        ),
        Splat(_) => r(&[band(420.0, 2.5, 1.0, 0.16), tone(260.0, 70.0, 0.1, 0.8, 0.18), later(band(1100.0, 3.0, 0.5, 0.12), 0.05)], 0.5),
        Sparks => r(&[high(4200.0, 0.6, 0.03), later(band(4300.0, 1.5, 0.4, 0.02), 0.05), later(high(5500.0, 0.3, 0.02), 0.11)], 0.4),
        Hit { head: false } => r(&[tone(1600.0, 1600.0, 0.0, 0.5, 0.03)], 0.0),
        Hit { head: true } => r(&[tone(2400.0, 2400.0, 0.0, 0.5, 0.03), later(tone(3200.0, 3200.0, 0.0, 0.4, 0.04), 0.05)], 0.0),
        Kill => r(&[tone(880.0, 880.0, 0.0, 0.5, 0.07), later(tone(1320.0, 1320.0, 0.0, 0.5, 0.12), 0.08)], 0.1),
        Hurt(_) => r(&[low(260.0, 1.0, 0.12), tone(120.0, 55.0, 0.06, 0.8, 0.1), band(900.0, 1.0, 0.2, 0.04)], 0.2),
        Death => r(&[tone(220.0, 55.0, 0.5, 0.8, 0.45), low(300.0, 0.8, 0.35), later(low(150.0, 0.5, 0.4), 0.1)], 0.8),
        Clank => r(&[band(1100.0, 8.0, 1.0, 0.09), band(2300.0, 6.0, 0.5, 0.06), band(3000.0, 1.0, 0.4, 0.008)], 0.4),
        Shutdown => r(&[from(tone(420.0, 50.0, 0.25, 0.6, 0.6), Src::Saw), band(5000.0, 1.0, 0.4, 0.05), later(band(4200.0, 1.2, 0.3, 0.04), 0.12), later(low(300.0, 0.6, 0.2), 0.05)], 0.6),
        Pickup(Pick::Health) => r(&[tone(660.0, 660.0, 0.0, 0.5, 0.12), later(tone(990.0, 990.0, 0.0, 0.45, 0.18), 0.07)], 0.3),
        Pickup(Pick::Armour) => r(&[band(1400.0, 6.0, 0.7, 0.08), from(tone(330.0, 330.0, 0.0, 0.3, 0.14), Src::Square), later(tone(495.0, 495.0, 0.0, 0.3, 0.16), 0.07)], 0.3),
        Pickup(Pick::Ammo) => r(&[band(2600.0, 2.0, 0.8, 0.012), later(band(2200.0, 2.0, 0.8, 0.012), 0.06), later(low(700.0, 0.4, 0.04), 0.06)], 0.2),
        Pickup(Pick::Weapon) => r(&[tone(523.0, 523.0, 0.0, 0.4, 0.1), later(tone(659.0, 659.0, 0.0, 0.4, 0.1), 0.07), later(tone(784.0, 784.0, 0.0, 0.45, 0.2), 0.14), band(1800.0, 3.0, 0.5, 0.02)], 0.3),
        Grab => r(&[tone(740.0, 740.0, 0.0, 0.4, 0.08), later(tone(990.0, 990.0, 0.0, 0.35, 0.1), 0.05)], 0.3),
        Switch => r(&[band(1800.0, 2.0, 0.8, 0.02), later(band(2600.0, 3.0, 0.7, 0.012), 0.09)], 0.15),
        ReloadOut => r(&[band(2200.0, 3.0, 0.8, 0.015), later(swept(band(900.0, 2.0, 0.5, 0.06), 1500.0, 0.05), 0.03)], 0.15),
        ReloadIn => r(&[band(1500.0, 2.0, 0.9, 0.03), later(band(3000.0, 3.0, 0.9, 0.015), 0.1), later(low(500.0, 0.4, 0.03), 0.1)], 0.15),
        // A step is a heel and then a toe 70 ms later (Cook; Turchet and
        // Nordahl), each a soft thud with a few milliseconds of onset, a
        // foot rather than a crack, and the ground's own texture over it:
        // a scuff on stone, a swish through grass, a crunch of leaves, a
        // slosh in water.
        Step(Ground::Stone) => r(
            &[slow(low(320.0, 0.9, 0.035), 0.004), slow(band(1200.0, 0.8, 0.22, 0.02), 0.003), later(slow(low(420.0, 0.5, 0.03), 0.004), 0.07), later(slow(band(1800.0, 0.8, 0.18, 0.035), 0.006), 0.07)],
            0.15,
        ),
        Step(Ground::Grass) => r(
            &[slow(low(250.0, 0.8, 0.04), 0.006), slow(band(3000.0, 0.6, 0.18, 0.06), 0.015), later(slow(low(320.0, 0.4, 0.03), 0.006), 0.07), later(slow(band(2600.0, 0.6, 0.15, 0.07), 0.02), 0.06)],
            0.05,
        ),
        Step(Ground::Leaves) => r(
            &[slow(low(260.0, 0.8, 0.04), 0.006), slow(band(2200.0, 0.7, 0.25, 0.05), 0.008), later(slow(band(3200.0, 0.8, 0.2, 0.06), 0.01), 0.05), later(slow(low(340.0, 0.4, 0.03), 0.006), 0.07)],
            0.05,
        ),
        Step(Ground::Water) => r(
            &[slow(low(300.0, 0.45, 0.05), 0.008), slow(band(900.0, 1.0, 0.2, 0.08), 0.012), tone(650.0, 1200.0, 0.04, 0.07, 0.05), later(slow(band(1300.0, 1.0, 0.15, 0.07), 0.015), 0.06)],
            0.05,
        ),
        Step(Ground::Soft) => r(&[slow(low(300.0, 0.9, 0.045), 0.006), slow(band(900.0, 0.8, 0.12, 0.03), 0.005), later(slow(low(380.0, 0.5, 0.035), 0.006), 0.07)], 0.05),
        Jump => r(&[slow(band(700.0, 1.2, 0.35, 0.08), 0.03)], 0.1),
        Land(_) => r(&[low(240.0, 1.0, 0.09), tone(90.0, 45.0, 0.05, 0.7, 0.08), band(1500.0, 1.0, 0.2, 0.02)], 0.3),
        Pad => r(&[tone(110.0, 420.0, 0.15, 0.8, 0.25), swept(slow(band(600.0, 1.5, 0.6, 0.25), 0.02), 2400.0, 0.2)], 0.5),
        Teleport => r(&[from(tone(300.0, 1400.0, 0.2, 0.4, 0.35), Src::Saw), swept(slow(band(1500.0, 2.0, 0.6, 0.3), 0.05), 5000.0, 0.25)], 0.6),
        Respawn => r(&[swept(slow(band(400.0, 2.0, 0.6, 0.4), 0.15), 3200.0, 0.4), tone(440.0, 880.0, 0.3, 0.3, 0.5)], 0.6),
        FlagTaken(true) | FlagCaptured(false) => r(&[from(tone(440.0, 440.0, 0.0, 0.4, 0.15), Src::Square), later(from(tone(330.0, 330.0, 0.0, 0.4, 0.3), Src::Square), 0.15)], 0.3),
        FlagTaken(false) => r(&[tone(523.0, 523.0, 0.0, 0.45, 0.12), later(tone(784.0, 784.0, 0.0, 0.45, 0.25), 0.12)], 0.3),
        FlagCaptured(true) => r(
            &[tone(523.0, 523.0, 0.0, 0.4, 0.12), later(tone(659.0, 659.0, 0.0, 0.4, 0.12), 0.12), later(tone(784.0, 784.0, 0.0, 0.4, 0.12), 0.24), later(tone(1047.0, 1047.0, 0.0, 0.5, 0.4), 0.36)],
            0.3,
        ),
        FlagDropped => r(&[tone(392.0, 330.0, 0.1, 0.4, 0.3)], 0.3),
        FlagReturned => r(&[tone(587.0, 587.0, 0.0, 0.4, 0.1), later(tone(587.0, 587.0, 0.0, 0.4, 0.15), 0.14)], 0.3),
        Dig(Ground::Stone) => r(&[band(2100.0, 1.0, 0.8, 0.03), low(600.0, 0.7, 0.05), later(band(3500.0, 1.0, 0.3, 0.06), 0.03)], 0.4),
        Dig(_) => r(&[low(800.0, 0.9, 0.06), band(1500.0, 0.8, 0.4, 0.08)], 0.3),
        Place => r(&[low(450.0, 0.9, 0.04), band(1300.0, 2.0, 0.5, 0.02)], 0.3),
    }
}

/// A noise as it reaches the ear: the recipe, how loud, which side, how
/// much air it came through, and how many samples late.
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub kind: Kind,
    pub gain: f32,
    pub pan: f32,
    pub cut: f32,
    pub delay: f32,
    pub pitch: f32,
    pub key: u32,
}

/// Where a noise lands for a listener at `eye` facing `facing` (x, z):
/// None if it is too far or too quiet to be worth a voice.
pub fn place(n: &Noise, eye: Vec3, facing: [f32; 2], pitch: f32) -> Option<Shot> {
    let passed;
    let n = match (n.kind, n.at) {
        (Kind::Whizz { from, .. }, Some(to)) => {
            passed = Noise { at: Some(nearest(from, to, eye)?), ..*n };
            &passed
        }
        _ => n,
    };
    let (gain, side, cut, delay) = match n.at {
        None => (n.gain, 0.0, 20_000.0, 0.0),
        Some(p) => {
            let to = p - eye;
            let d = to.length();
            // Inverse distance from a couple of metres out; louder things
            // carry further only by being louder.
            // A burst is heard from further: it is the loudest thing in a
            // match, and at 2.5 m a rocket going off across the room was
            // quieter than the gun that fired it.
            let near = match n.kind {
                Kind::Burst(r) => (r * 2.0).max(2.5),
                _ => 2.5,
            };
            let att = near / d.max(near);
            let flat = glam::Vec2::new(to.x, to.z);
            let fl = glam::Vec2::new(facing[0], facing[1]).normalize_or(glam::Vec2::NEG_Y);
            let right = glam::Vec2::new(-fl.y, fl.x);
            // Close to the ear a sound is nearly central whatever its bearing.
            let side = if flat.length() > 0.3 { flat.normalize().dot(right) * (d / (d + 1.5)) } else { 0.0 };
            // Behind the listener, a little duller.
            let behind = if flat.length() > 0.3 && flat.normalize().dot(fl) < 0.0 { 0.75 } else { 1.0 };
            (n.gain * att, side, 18_000.0 / (1.0 + d / 60.0) * behind, d / 343.0)
        }
    };
    if gain < 0.004 {
        return None;
    }
    Some(Shot { kind: n.kind, gain: gain.min(2.0), pan: side, cut, delay, pitch: pitch * n.pitch, key: n.key })
}

/// Metres from the ear inside which a round is heard going past.
const WHIZZ_M: f32 = 5.0;

/// Where a round's line from `from` to `to` came nearest `eye`, if it went
/// past close enough to hear. Not if it stopped short (or in the listener:
/// that is a hit, and heard as one), and not if the nearest point is at the
/// muzzle: that is the listener's own round.
fn nearest(from: Vec3, to: Vec3, eye: Vec3) -> Option<Vec3> {
    let ab = to - from;
    let t = ((eye - from).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    let p = from + ab * t;
    let close = (eye - p).length() < WHIZZ_M;
    let passed = t < 0.999 && (p - to).length() > 1.0;
    let mine = (p - from).length() < 3.0;
    (close && passed && !mine).then_some(p)
}

struct LayerState {
    phase: f32,
    filt: Biquad,
    rng: Rng,
}

struct Voice {
    kind: Kind,
    pan: f32,
    recipe: Recipe,
    st: [LayerState; LAYERS],
    /// Seconds since the voice began sounding (after its delay).
    t: f32,
    /// Samples still to wait before it sounds.
    wait: usize,
    gain: f32,
    lr: (f32, f32),
    air: [OnePole; 2],
    pitch: f32,
    key: u32,
    /// Held voices: seconds it stays up without another post; and how far
    /// through letting go it is (1 still held, falling to 0).
    alive: f32,
    hold: bool,
    release: f32,
    /// How loud its own envelope still is, for choosing a voice to steal.
    level: f32,
    size: f32,
}

/// How long a held voice stays up after the last post: a little over two
/// steps, so one dropped step does not cut a beam.
const HOLD_S: f32 = 0.06;

/// The pool of one-shot voices.
pub struct Fx {
    voices: Vec<Voice>,
    rng: Rng,
}

pub const VOICES: usize = 40;

impl Fx {
    pub fn new() -> Fx {
        Fx { voices: Vec::with_capacity(VOICES), rng: Rng::new(0xF00D) }
    }

    /// Start a noise (or keep a held one up).
    pub fn start(&mut self, s: Shot, sr: f32) {
        let hold = matches!(s.kind, Kind::Beam(_) | Kind::Rush);
        let lr = pan(s.pan);
        if hold {
            if let Some(v) = self.voices.iter_mut().find(|v| v.hold && v.key == s.key && v.release >= 1.0) {
                v.alive = HOLD_S;
                v.gain = s.gain;
                v.lr = lr;
                v.pitch = s.pitch;
                for a in &mut v.air {
                    a.set(s.cut, sr);
                }
                return;
            }
        }
        // A beam held on a wall strikes it every step, and a scatter gun's
        // pellets all land at once: one impact a moment is the sound of
        // either, and twelve are a buzz.
        let (gap, kin) = match s.kind {
            Kind::Impact(_) => (0.07, true),
            Kind::Ricochet => (0.3, true),
            Kind::Sparks | Kind::Bounce => (0.05, true),
            // A scatter gun's pellets all go past at once.
            Kind::Whizz { .. } => (0.04, true),
            _ => (0.0, false),
        };
        let same = |k: &Kind| std::mem::discriminant(k) == std::mem::discriminant(&s.kind);
        if kin && self.voices.iter().any(|v| same(&v.kind) && v.t + v.wait as f32 / sr < gap && (v.pan - s.pan).abs() < 0.25) {
            return;
        }
        if self.voices.len() >= VOICES {
            // The quietest goes; a pile-up of shots loses its tails first.
            let (i, _) = self.voices.iter().enumerate().min_by(|a, b| (a.1.level * a.1.gain).total_cmp(&(b.1.level * b.1.gain))).unwrap();
            self.voices.swap_remove(i);
        }
        let recipe = recipe(s.kind);
        // No two rounds alike: a little louder or softer each time, as the
        // pitch already varies.
        let gain = if matches!(s.kind, Kind::Shot(_)) { s.gain * (0.85 + 0.3 * self.rng.unit()) } else { s.gain };
        let size = match s.kind {
            Kind::Burst(r) | Kind::Splat(r) => (r / 3.0).clamp(0.5, 2.0),
            Kind::Hurt(h) => 0.7 + h.clamp(0.0, 1.0),
            Kind::Land(v) => (v / 12.0).clamp(0.3, 1.5),
            _ => 1.0,
        };
        let mut air = [OnePole::default(), OnePole::default()];
        for a in &mut air {
            a.set(s.cut, sr);
        }
        let seed = self.rng.next();
        self.voices.push(Voice {
            kind: s.kind,
            pan: s.pan,
            recipe,
            st: std::array::from_fn(|k| LayerState { phase: 0.0, filt: Biquad::default(), rng: Rng::new(seed.wrapping_add(k as u32 * 7919)) }),
            t: 0.0,
            wait: (s.delay * sr) as usize,
            gain,
            lr,
            air,
            // A bigger burst is a lower one.
            pitch: s.pitch / size.sqrt(),
            key: s.key,
            alive: HOLD_S,
            hold,
            release: 1.0,
            level: 1.0,
            size,
        });
    }

    /// Run every voice for one block, adding into `l`, `r` and the reverb
    /// send `send`.
    pub fn run(&mut self, l: &mut [f32], r: &mut [f32], send: &mut [f32], sr: f32) {
        let n = l.len();
        let dt = n as f32 / sr;
        let mut buf = [0.0f32; super::BLOCK];
        self.voices.retain_mut(|v| {
            // Waiting for the sound to arrive.
            let mut first = 0;
            if v.wait >= n {
                v.wait -= n;
                return true;
            } else if v.wait > 0 {
                first = v.wait;
                v.wait = 0;
            }
            let span = (n - first) as f32 / sr;
            let mut loudest = 0.0f32;
            let mut ended = true;
            let b = &mut buf[..n];
            b.fill(0.0);
            for (k, ly) in v.recipe.layers().iter().enumerate() {
                let st = &mut v.st[k];
                let t0 = v.t - ly.at;
                let t1 = t0 + span;
                // Hold layers stay at full while held.
                let env = |t: f32| -> f32 {
                    if t < 0.0 {
                        0.0
                    } else if t < ly.attack {
                        t / ly.attack
                    } else if v.hold {
                        1.0
                    } else {
                        (-(t - ly.attack) / (ly.decay * v.size.max(0.5))).exp()
                    }
                };
                let (e0, e1) = (env(t0) * ly.amp, env(t1) * ly.amp);
                if t1 < 0.0 || e1 > 1e-3 || v.hold {
                    ended = false;
                }
                loudest = loudest.max(e1);
                if e0 <= 0.0 && e1 <= 0.0 {
                    continue;
                }
                let glide = |a: f32, z: f32| if ly.glide > 0.0 { z + (a - z) * (-t0.max(0.0) / ly.glide).exp() } else { a };
                let hz = glide(ly.hz[0], ly.hz[1]) * v.pitch;
                match ly.filt {
                    Filt::Off => {}
                    Filt::Lp => st.filt.lowpass(glide(ly.cut[0], ly.cut[1]) * v.pitch, ly.q, sr),
                    Filt::Bp => st.filt.bandpass(glide(ly.cut[0], ly.cut[1]) * v.pitch, ly.q, sr),
                    Filt::Hp => st.filt.highpass(glide(ly.cut[0], ly.cut[1]) * v.pitch, ly.q, sr),
                }
                let step = hz / sr;
                let m = (n - first) as f32;
                // A crack is over in a few milliseconds, shorter than a
                // block: early in a layer the envelope is worked out at
                // every sample, after that a straight line across the block
                // is close enough.
                let exact = t0 < ly.attack + 0.03;
                for (i, o) in b.iter_mut().enumerate().skip(first) {
                    let k = (i - first) as f32;
                    let e = if exact { env(t0 + k / sr) * ly.amp } else { e0 + (e1 - e0) * (k / m) };
                    let x = match ly.src {
                        Src::Noise => st.rng.bi(),
                        Src::Sine => (st.phase * std::f32::consts::TAU).sin(),
                        Src::Saw => 2.0 * st.phase - 1.0,
                        Src::Square => {
                            if st.phase < 0.5 {
                                0.7
                            } else {
                                -0.7
                            }
                        }
                        Src::Pulse => {
                            let u = (t0 + k / sr).max(0.0) * hz;
                            (1.0 - u) * (-1.5 * u).exp()
                        }
                    };
                    st.phase = (st.phase + step).fract();
                    let y = match ly.filt {
                        Filt::Off => x,
                        _ => st.filt.run(x),
                    };
                    *o += y * e;
                }
            }
            v.t += span;
            v.level = loudest;
            // A held voice lets go when its posts stop, over a tenth of a
            // second.
            let rel0 = v.release;
            if v.hold {
                v.alive -= dt;
                if v.alive <= 0.0 {
                    v.release = (v.release - dt / 0.1).max(0.0);
                }
                if v.release <= 0.0 {
                    return false;
                }
            } else if ended {
                return false;
            }
            let send_k = v.recipe.send;
            // Driven into a soft clipper: the peaks flatten, the quiet
            // parts come up, and a clean thump turns into a report.
            let d = v.recipe.drive;
            if d > 0.0 {
                let norm = 1.0 / d.tanh();
                for x in &mut b[first..] {
                    *x = (*x * d).tanh() * norm;
                }
            }
            for i in first..n {
                let rel = if v.hold {
                    let r = rel0 + (v.release - rel0) * ((i - first) as f32 / (n - first) as f32);
                    r * r
                } else {
                    1.0
                };
                let x = b[i] * v.gain * rel;
                let xl = v.air[0].lp(x * v.lr.0);
                let xr = v.air[1].lp(x * v.lr.1);
                l[i] += xl;
                r[i] += xr;
                send[i] += (xl + xr) * 0.5 * send_k;
            }
            true
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Library;

    /// Every gun in the library gets a report of its own kind, and the ones
    /// that should differ do.
    #[test]
    fn every_gun_has_a_report() {
        let (lib, _) = Library::load(&crate::library::data_root());
        let of = |name: &str, alt: bool| {
            let w = &lib.weapons[lib.weapon(name).unwrap_or_else(|| panic!("no {name}"))];
            let f = if alt { w.alt.as_ref().unwrap() } else { &w.fire };
            Report::of(f)
        };
        assert_eq!(of("sidearm", false), Report::Pistol);
        assert_eq!(of("minigun", false), Report::Minigun);
        assert_eq!(of("sniper_rifle", false), Report::Sniper);
        assert_eq!(of("rocket_launcher", false), Report::Rocket);
        assert_eq!(of("goo_gun", false), Report::Goo);
        assert_eq!(of("skipjack", false), Report::Disc);
        assert_eq!(of("scrap_gun", false), Report::Scatter);
        assert_eq!(of("pulse_gun", false), Report::Pulse);
        assert_eq!(of("bore", false), Report::Bore);
        assert_eq!(of("digger", false), Report::Swing);
    }
}
